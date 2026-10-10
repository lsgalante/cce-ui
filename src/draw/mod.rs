//! What a renderer draws, with no renderer in it: the frame
//! ([`Frame2D`] and its [`Batch2D`]s, [`PlatePush`] parameter blocks and the
//! one way to lay a batch's block out, [`batch_push_constants`]), text runs
//! ([`TextSpan`]), image draws ([`ImageQuad`]) and the image-id queue apps
//! upload through ([`images`]).
//!
//! These lived in `vk/` because the Vulkan renderer was the only one. They
//! are plain data, and `backend::frame` builds them for any renderer — the
//! Vulkan one on Linux, a WebGPU one in the browser — so they live here, and
//! `vk` re-exports every one at its old path.

/// One 2D vertex of a tessellated frame: what the 2D shaders take. Defined
/// here, with the rest of what a renderer draws; `backend::tessellate` makes
/// it and re-exports it at its old path.
#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Vertex {
    pub position: [f32; 2],
    pub color: [f32; 4],
    pub clip_circle: [f32; 3], // [cx, cy, r]
}
use cosmic_text::Buffer as TextBuffer;

pub mod glyphs;
pub mod images;
pub mod lit;
pub mod rt;
pub mod scene;
pub mod shaders;

pub use images::{
    free_image, recycle_buffer, renderer_epoch, update_pixel_regions, update_pixels,
    upload_pixels, upload_rgba, upload_rgba_mipmapped, ImageQuad, PixelFormat, Region,
};

/// One scissored draw range of a 2D frame. `scissor` is (x, y, w, h) in
/// physical pixels; None draws with the full-surface scissor. `clip_rrect` is an
/// optional rounded-rect clip `[cx, cy, bx, by, r]` (center, SDF half-extents, corner
/// radius; physical px) applied via push constants — fragments outside it discard, so a
/// plate's children cut off at its rounded corners.
pub struct Batch2D {
    pub scissor: Option<(u32, u32, u32, u32)>,
    pub clip_rrect: Option<[f32; 5]>,
    pub start: u32,
    pub end: u32,
    /// When set, this batch is a single SDF-lit plate cover quad: the params go
    /// out as push constants and shader2d's plate branch lights it per pixel.
    pub plate: Option<PlatePush>,
    /// A blur-behind plate (negative-alpha color): the renderer suspends the UI
    /// pass, copies the swapchain-so-far into its snapshot image, and resumes —
    /// so the plate's blur samples everything painted beneath it (background,
    /// widgets, wires), not just the 3D scene backdrop.
    pub blur_behind: bool,
}

/// Floats in the fragment push-constant block: the rounded-rect clip (`rect0`,
/// `rect1` — 6 clip/flag floats plus the plate mode and corner shape) followed
/// by [`PlatePush`]'s six vec4s. Field for field, this is shader2d's `RRectClip`.
pub const PUSH_CONSTANT_FLOATS: usize = 32;

/// The fragment push-constant block for one batch: its rounded-rect clip,
/// and its plate block when it is a plate cover quad. `feature_base` is the
/// frame slot's first entry in the feature UBO, added to a plate's (or a
/// union carve's) feature offset. Shared by every renderer and the offscreen
/// test harness (`vk::plate_probe`), so none can push a different block. (Vulkan
/// carries it as push constants; a renderer without them puts it in a uniform.)
pub fn batch_push_constants(batch: &Batch2D, clip_shape: f32, feature_base: usize) -> [f32; PUSH_CONSTANT_FLOATS] {
    let rr = batch.clip_rrect.unwrap_or([0.0; 5]);
    let enabled = if batch.clip_rrect.is_some() { 1.0f32 } else { 0.0 };
    let mut pc = [0.0f32; PUSH_CONSTANT_FLOATS];
    pc[..5].copy_from_slice(&rr);
    pc[5] = enabled;
    pc[7] = clip_shape;
    if let Some(p) = &batch.plate {
        pc[6] = p.mode;
        pc[7] = p.shape;
        pc[8..12].copy_from_slice(&p.rect);
        pc[12..16].copy_from_slice(&p.radii);
        pc[16..20].copy_from_slice(&p.light);
        pc[20..24].copy_from_slice(&p.material);
        pc[24..28].copy_from_slice(&p.host);
        pc[28..32].copy_from_slice(&p.specular_tint);
        if p.mode == 1.0 || p.mode == 14.0 || p.mode == 17.0 {
            // Rebase the feature offset onto this frame's UBO slot (a plate's
            // CSG carves, or a union carve's boxes).
            pc[24] += feature_base as f32;
        }
    }
    pc
}

/// Push-constant block for one SDF-lit plate batch (physical px throughout).
/// Mirrors the `p_*` fields of shader2d's `RRectClip`.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct PlatePush {
    /// SDF box: center + half-extents. May extend past the cover quad — that is
    /// how a recess suppresses a wall.
    pub rect: [f32; 4],
    /// Per-corner radii [tl, tr, br, bl].
    pub radii: [f32; 4],
    /// xyz = unit vector toward the light (+z out of the screen), w = roll width px.
    pub light: [f32; 4],
    /// [shading strength, specular strength, shininess, curvature/AO strength].
    pub material: [f32; 4],
    /// Mode 1: `[feature offset, feature count, frost z, frost w]` — xy into
    /// the frame's `plate_features`, the carves CSG'd out of this plate (the
    /// renderer adds the frame slot's base offset at record time); zw the
    /// plate's frost recipe, `scene::material::Frost::pack` (compression and
    /// refraction packed in z, the blur sigma in physical px in w). Mode 14 uses the same
    /// `[offset, count]` for the union's boxes. Mode 2: the host-plate box
    /// (center + half-extents) a free recess fades out against; far-away sides
    /// (±1e5) disable the fade.
    pub host: [f32; 4],
    /// RGB multiplies the roll's specular color. Neutral white normally; the
    /// focused-pane bevel carries the highlight color here, with w = 1 marking
    /// the plate (or carve) as accent-tinted — the shader's focus branch,
    /// which recolours the light AND the shadow (see shader2d's FOCUS_*).
    pub specular_tint: [f32; 4],
    /// 1.0 = raised lit plate, 2.0 = recess overlay, 3.0 = boss, 4.0 = ridge,
    /// 5.0 = sphere, 6.0/7.0 = concave fillet (recessed/raised), 8.0 = groove
    /// (slab carve about a line: `rect` = [cx, cy, half-width, _], `radii.xy` =
    /// the line's unit normal, `host` = the surface it is engraved into),
    /// 9.0 = trough, 10.0 = droplet (`radii` = [sag, belly r, belly half-w,
    /// blend k] px, `host` = [sheet corner r px, clarity, dome amplitude,
    /// attach r px], `material.w` = fresnel rim, `specular_tint` = [core
    /// density, _, _, bottom-bow rise px] — droplet glints are always white,
    /// so the tint RGB is repurposed; see shader2d's MODE_DROPLET).
    pub mode: f32,
    /// Corner shape exponent: 2.0 = circular arcs, > 2 = superellipse
    /// (continuous-curvature) corners — see shader2d's `plate_sdf_grad`.
    pub shape: f32,
}

/// A full 2D frame: the display-list vertices (optionally split into scissored
/// batches), overlay vertices drawn after text, and the clear color (linear;
/// only used on frames without a backdrop copy).
pub struct Frame2D<'a> {
    pub verts: &'a [Vertex],
    pub batches: &'a [Batch2D],
    pub overlay_verts: &'a [Vertex],
    /// User images drawn interleaved with `verts` by each quad's `z_before`.
    pub images: &'a [ImageQuad],
    /// Carves CSG'd into this frame's SDF-lit plates, 12 floats each (rect
    /// center+half-extents, per-corner radii, [width px, depth px, 0, 0]).
    /// Plate batches reference them by offset+count in `PlatePush::host`.
    pub plate_features: &'a [[f32; 12]],
    pub clear_color: [f32; 4],
    /// The only part of the surface that differs from the previous frame,
    /// (x, y, w, h) in physical pixels; None = all of it. With a rect the
    /// renderer keeps the pixels outside it (Vulkan's `ImageAge`) and tells the
    /// compositor that only the rect changed. The caller vouches for it: a
    /// pixel that changed outside the rect stays as it was.
    pub damage: Option<(u32, u32, u32, u32)>,
}

/// Max plate-carve features per frame; the shader's UBO holds one slot of this
/// size per frame in flight.
pub const MAX_PLATE_FEATURES: usize = 64;

/// One shaped text run to draw. `left`/`top` are physical pixels and `scale`
/// multiplies the shaped (logical) glyph positions — the same contract as
/// the old glyphon::TextArea, where callers pass `label.x * scale`.
pub struct TextSpan<'a> {
    pub buffer: &'a TextBuffer,
    pub left: f32,
    pub top: f32,
    pub scale: f32,
    /// Physical-pixel clip rect (left, top, right, bottom); None = whole surface.
    pub bounds: Option<[i32; 4]>,
    /// 0..=1 sRGB + alpha, applied to glyphs without their own color.
    pub default_color: [f32; 4],
    /// Rotate the span's glyph quads by (radians, center_x, center_y) in
    /// physical pixels — the circular network pane's curved rim labels.
    pub rotation: Option<(f32, f32, f32)>,
    /// Fragment circle clip (center_x, center_y, radius) in physical pixels;
    /// zero radius disables (matches shader.wgsl's clip_circle).
    pub clip_circle: [f32; 3],
    /// Rounded-rect clip half-extents (physical px). Zero keeps `clip_circle` a plain
    /// circle; non-zero reinterprets it as a rounded-rect SDF clip — center
    /// `clip_circle.xy`, corner radius `clip_circle.z`, inner box half-size
    /// `clip_extents` — so plate children (labels included) cut off at rounded corners.
    pub clip_extents: [f32; 2],
}

/// The bytes of one plate feature (a carve CSG'd into a plate): three vec4s,
/// `[f32; 12]` in `Frame2D::plate_features`.
pub const PLATE_FEATURE_BYTES: usize = 48;

/// shader2d's WindowInfo UBO: [size/clip vec4][bevel-profile meta vec4]
/// [8 vec4 of profile slope samples].
// [size/clip vec4][carve profile meta + 8 vec4][roll profile meta + 8 vec4].
// [size/clip vec4][carve profile meta][8 carve slopes][roll profile meta]
// [8 roll slopes][relief heights][backdrop meta] = 21 vec4. Grows only at the
// END — every offset above is addressed by index from both sides.
pub const WINDOW_INFO_BYTES: usize = 320;

/// The pinned relief heights (carve, roll) in physical px at `scale`, 0 =
/// follow the width.
pub fn relief_px_at(scale: f32) -> (f32, f32) {
    let s = scale.max(0.001);
    (
        crate::layout::bevel_height().map_or(0.0, |h| h * s),
        crate::layout::roll_height().map_or(0.0, |h| h * s),
    )
}

/// shader2d's `WindowInfo` block for a `width` x `height` target whose corners clip
/// at `clip_corner_radius` (physical px), with the pinned relief heights
/// `relief` (carve, roll; physical px, 0 = unpinned) — the profiles and the
/// corner shape from the live style. Shared by every renderer and the offscreen
/// test harness.
pub fn window_info_data(width: u32, height: u32, clip_corner_radius: f32, relief: (f32, f32)) -> [f32; WINDOW_INFO_BYTES / 4] {
    let mut data = [0.0f32; WINDOW_INFO_BYTES / 4];
    data[0] = width as f32;
    data[1] = height as f32;
    data[2] = clip_corner_radius;
    data[3] = crate::layout::corner_shape();
    if let Some(slopes) = crate::layout::bevel_profile_slopes() {
        data[4] = 1.0;
        data[5] = crate::layout::BEVEL_PROFILE_SAMPLES as f32;
        data[8..8 + slopes.len()].copy_from_slice(&slopes);
    }
    if let Some(slopes) = crate::layout::roll_profile_slopes() {
        data[40] = 1.0;
        data[41] = crate::layout::BEVEL_PROFILE_SAMPLES as f32;
        data[44..44 + slopes.len()].copy_from_slice(&slopes);
    }
    data[76] = relief.0;
    data[77] = relief.1;
    data
}
