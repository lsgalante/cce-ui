//! The glyph atlas, CPU side: shaped runs ([`TextSpan`]s) rasterized through
//! swash into an RGBA8 atlas (a shelf packer, a cache keyed by cosmic-text's
//! glyph key) and turned into glyph quads. A renderer uploads [`pixels`]
//! whenever [`generation`] moves and draws [`vertices`] with `glyph.wgsl`.
//! Image quads use the same vertex and shader ([`image_quad_vertices`]).
//!
//! It was half of the Vulkan text stage; the other half — the upload and
//! the draw — is what each renderer keeps.
//!
//! [`pixels`]: GlyphAtlas::pixels
//! [`generation`]: GlyphAtlas::generation
//! [`vertices`]: GlyphAtlas::vertices

use std::collections::HashMap;

use cosmic_text::{CacheKey, FontSystem, SwashCache, SwashContent};

use super::{ImageQuad, TextSpan};

/// The atlas is one ATLAS_SIZE² RGBA8 texture (unorm, sampled nearest).
pub const ATLAS_SIZE: u32 = 1024;
const ATLAS_PAD: u32 = 1;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct GlyphVertex {
    pub position: [f32; 2],
    pub uv: [f32; 2],
    pub color: [f32; 4],
    pub clip_circle: [f32; 3],
    pub clip_extents: [f32; 2],
}

// The glyph shader's vertex (locations 0..=4) for text AND image quads: both
// pipelines feed `glyph.wgsl`, and every renderer describes this exact layout.
const _: () = {
    assert!(std::mem::size_of::<GlyphVertex>() == 52);
    assert!(std::mem::offset_of!(GlyphVertex, position) == 0);
    assert!(std::mem::offset_of!(GlyphVertex, uv) == 8);
    assert!(std::mem::offset_of!(GlyphVertex, color) == 16);
    assert!(std::mem::offset_of!(GlyphVertex, clip_circle) == 32);
    assert!(std::mem::offset_of!(GlyphVertex, clip_extents) == 44);
};

#[derive(Clone, Copy)]
struct GlyphEntry {
    /// Atlas texel rect.
    u: u32,
    v: u32,
    w: u32,
    h: u32,
    /// Raster placement offsets (from swash).
    left: i32,
    top: i32,
    is_color: bool,
    /// Zero-sized raster (spaces): nothing to draw, but cached to skip re-rastering.
    empty: bool,
}

struct Shelf {
    cursor_x: u32,
    cursor_y: u32,
    row_height: u32,
}

impl Shelf {
    fn new() -> Self {
        Shelf { cursor_x: ATLAS_PAD, cursor_y: ATLAS_PAD, row_height: 0 }
    }

    fn insert(&mut self, w: u32, h: u32) -> Option<(u32, u32)> {
        if w > ATLAS_SIZE - 2 * ATLAS_PAD || h > ATLAS_SIZE - 2 * ATLAS_PAD {
            return None;
        }
        if self.cursor_x + w + ATLAS_PAD > ATLAS_SIZE {
            self.cursor_x = ATLAS_PAD;
            self.cursor_y += self.row_height + ATLAS_PAD;
            self.row_height = 0;
        }
        if self.cursor_y + h + ATLAS_PAD > ATLAS_SIZE {
            return None;
        }
        let pos = (self.cursor_x, self.cursor_y);
        self.cursor_x += w + ATLAS_PAD;
        self.row_height = self.row_height.max(h);
        Some(pos)
    }
}

/// The atlas and this frame's glyph quads.
pub struct GlyphAtlas {
    /// RGBA8, ATLAS_SIZE², the texture's contents.
    pixels: Vec<u8>,
    /// Bumped whenever `pixels` changes; a renderer re-uploads on a change.
    generation: u64,
    glyphs: HashMap<CacheKey, GlyphEntry>,
    shelf: Shelf,
    /// The staged text: replaced only by the next [`prepare`](Self::prepare),
    /// so a frame drawn without a re-prepare keeps its text.
    vertices: Vec<GlyphVertex>,
}

impl Default for GlyphAtlas {
    fn default() -> Self {
        Self::new()
    }
}

impl GlyphAtlas {
    pub fn new() -> Self {
        Self {
            pixels: vec![0u8; (ATLAS_SIZE * ATLAS_SIZE * 4) as usize],
            generation: 1,
            glyphs: HashMap::new(),
            shelf: Shelf::new(),
            vertices: Vec::new(),
        }
    }

    /// The atlas texture's contents (RGBA8, `ATLAS_SIZE`²).
    pub fn pixels(&self) -> &[u8] {
        &self.pixels
    }

    /// Moves whenever [`pixels`](Self::pixels) changed.
    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// The glyph quads the last [`prepare`](Self::prepare) built, six vertices each.
    pub fn vertices(&self) -> &[GlyphVertex] {
        &self.vertices
    }

    /// Rasterize (on miss) and cache one glyph. Returns None when the atlas is full.
    fn ensure_glyph(
        &mut self,
        font_system: &mut FontSystem,
        swash_cache: &mut SwashCache,
        key: CacheKey,
    ) -> Option<GlyphEntry> {
        if let Some(entry) = self.glyphs.get(&key) {
            return Some(*entry);
        }
        let image = swash_cache.get_image_uncached(font_system, key)?;
        let w = image.placement.width;
        let h = image.placement.height;
        if w == 0 || h == 0 || image.data.is_empty() {
            let entry = GlyphEntry {
                u: 0, v: 0, w: 0, h: 0, left: 0, top: 0, is_color: false, empty: true,
            };
            self.glyphs.insert(key, entry);
            return Some(entry);
        }
        let (u, v) = self.shelf.insert(w, h)?;

        let is_color = !matches!(image.content, SwashContent::Mask);
        for row in 0..h {
            for col in 0..w {
                let dst = (((v + row) * ATLAS_SIZE + (u + col)) * 4) as usize;
                let texel = match image.content {
                    SwashContent::Mask => {
                        let a = image.data[(row * w + col) as usize];
                        [255, 255, 255, a]
                    }
                    // Color and SubpixelMask rasters are RGBA.
                    _ => {
                        let src = ((row * w + col) * 4) as usize;
                        [
                            image.data[src],
                            image.data[src + 1],
                            image.data[src + 2],
                            image.data[src + 3],
                        ]
                    }
                };
                self.pixels[dst..dst + 4].copy_from_slice(&texel);
            }
        }
        self.generation += 1;

        let entry = GlyphEntry {
            u,
            v,
            w,
            h,
            left: image.placement.left,
            top: image.placement.top,
            is_color,
            empty: false,
        };
        self.glyphs.insert(key, entry);
        Some(entry)
    }

    /// Build this frame's glyph vertices. Positions/bounds in physical pixels,
    /// NDC computed against the target's
    /// `width` x `height` (Y up; the Vulkan renderer flips with its viewport).
    pub fn prepare(
        &mut self,
        font_system: &mut FontSystem,
        swash_cache: &mut SwashCache,
        spans: &[TextSpan<'_>],
        width: u32,
        height: u32,
    ) {
        self.vertices.clear();
        if !self.try_prepare(font_system, swash_cache, spans, width, height) {
            // Atlas full: clear and repack with only the glyphs this frame needs.
            log::info!("glyph atlas full — clearing and repacking");
            self.glyphs.clear();
            self.shelf = Shelf::new();
            self.pixels.fill(0);
            self.generation += 1;
            self.vertices.clear();
            if !self.try_prepare(font_system, swash_cache, spans, width, height) {
                log::error!("glyph atlas full even after repack; text truncated this frame");
            }
        }
    }

    fn try_prepare(
        &mut self,
        font_system: &mut FontSystem,
        swash_cache: &mut SwashCache,
        spans: &[TextSpan<'_>],
        width: u32,
        height: u32,
    ) -> bool {
        let sw = width as f32;
        let sh = height as f32;
        for span in spans {
            for run in span.buffer.layout_runs() {
                let line_y = (run.line_y * span.scale).round() as i32;
                for glyph in run.glyphs.iter() {
                    let physical = glyph.physical((span.left, span.top), span.scale);
                    let Some(entry) =
                        self.ensure_glyph(font_system, swash_cache, physical.cache_key)
                    else {
                        // Distinguish "atlas full" (retryable) from "unrasterizable"
                        // (skip): a missing swash image caches as empty above, so a
                        // None here means the shelf rejected it.
                        if swash_cache
                            .get_image_uncached(font_system, physical.cache_key)
                            .is_some()
                        {
                            return false;
                        }
                        continue;
                    };
                    if entry.empty {
                        continue;
                    }

                    // glyphon's placement formula (kept verbatim), physical pixels.
                    let mut x0 = (physical.x + entry.left) as f32;
                    let mut y0 = (line_y + physical.y - entry.top) as f32;
                    let mut x1 = x0 + entry.w as f32;
                    let mut y1 = y0 + entry.h as f32;
                    let mut u0 = entry.u as f32;
                    let mut v0 = entry.v as f32;
                    let mut u1 = u0 + entry.w as f32;
                    let mut v1 = v0 + entry.h as f32;

                    // CPU clip to span bounds, shrinking UVs proportionally.
                    if let Some([bl, bt, br, bb]) = span.bounds {
                        let (bl, bt, br, bb) = (bl as f32, bt as f32, br as f32, bb as f32);
                        if x0 >= br || x1 <= bl || y0 >= bb || y1 <= bt {
                            continue;
                        }
                        if x0 < bl {
                            u0 += bl - x0;
                            x0 = bl;
                        }
                        if x1 > br {
                            u1 -= x1 - br;
                            x1 = br;
                        }
                        if y0 < bt {
                            v0 += bt - y0;
                            y0 = bt;
                        }
                        if y1 > bb {
                            v1 -= y1 - bb;
                            y1 = bb;
                        }
                    }

                    let color = if entry.is_color {
                        [1.0, 1.0, 1.0, 1.0]
                    } else if let Some(c) = glyph.color_opt {
                        [
                            c.r() as f32 / 255.0,
                            c.g() as f32 / 255.0,
                            c.b() as f32 / 255.0,
                            c.a() as f32 / 255.0,
                        ]
                    } else {
                        span.default_color
                    };

                    // Corner positions, optionally rotated about the span's center
                    // (physical px) before the NDC mapping.
                    let corners = match span.rotation {
                        None => [[x0, y0], [x1, y0], [x0, y1], [x1, y1]],
                        Some((angle, cx, cy)) => {
                            let (sin_a, cos_a) = angle.sin_cos();
                            let rot = |px: f32, py: f32| {
                                let (dx, dy) = (px - cx, py - cy);
                                [cx + dx * cos_a - dy * sin_a, cy + dx * sin_a + dy * cos_a]
                            };
                            [rot(x0, y0), rot(x1, y0), rot(x0, y1), rot(x1, y1)]
                        }
                    };
                    let ndc = |p: [f32; 2]| {
                        [(p[0] / sw) * 2.0 - 1.0, 1.0 - (p[1] / sh) * 2.0]
                    };
                    let uv = |u: f32, v: f32| [u / ATLAS_SIZE as f32, v / ATLAS_SIZE as f32];
                    let clip_circle = span.clip_circle;
                    let clip_extents = span.clip_extents;
                    let tl = GlyphVertex { position: ndc(corners[0]), uv: uv(u0, v0), color, clip_circle, clip_extents };
                    let tr = GlyphVertex { position: ndc(corners[1]), uv: uv(u1, v0), color, clip_circle, clip_extents };
                    let bl = GlyphVertex { position: ndc(corners[2]), uv: uv(u0, v1), color, clip_circle, clip_extents };
                    let br = GlyphVertex { position: ndc(corners[3]), uv: uv(u1, v1), color, clip_circle, clip_extents };
                    self.vertices.extend([tl, tr, bl, tr, br, bl]);
                }
            }
        }
        true
    }
}

/// Image quads as the glyph shader's vertices, six per image in `images`
/// order, NDC against a `width` x `height` target. A renderer draws image `i`
/// as vertices `6i..6i+6` with that image's texture bound.
pub fn image_quad_vertices(images: &[ImageQuad], width: u32, height: u32) -> Vec<GlyphVertex> {
    let sw = width as f32;
    let sh = height as f32;
    let mut verts: Vec<GlyphVertex> = Vec::with_capacity(images.len() * 6);
    for q in images {
        let (x, y, w, h) = q.rect;
        let ndc = |px: f32, py: f32| [(px / sw) * 2.0 - 1.0, 1.0 - (py / sh) * 2.0];
        let color = [1.0, 1.0, 1.0, q.alpha];
        let clip_circle = [0.0; 3];
        // Zero extents = the shader's plain-circle clip degenerate case. Inert
        // while clip_circle.z is 0 (the clip branch never runs), but it must be
        // a defined value, not whatever a missing attribute would read.
        let clip_extents = [0.0; 2];
        let tl = GlyphVertex { position: ndc(x, y), uv: [0.0, 0.0], color, clip_circle, clip_extents };
        let tr = GlyphVertex { position: ndc(x + w, y), uv: [1.0, 0.0], color, clip_circle, clip_extents };
        let bl = GlyphVertex { position: ndc(x, y + h), uv: [0.0, 1.0], color, clip_circle, clip_extents };
        let br = GlyphVertex { position: ndc(x + w, y + h), uv: [1.0, 1.0], color, clip_circle, clip_extents };
        verts.extend([tl, tr, bl, tr, br, bl]);
    }
    verts
}
