use std::time::Instant;
use smithay_client_toolkit::{
    compositor::{CompositorHandler, CompositorState},
    data_device_manager::DataDeviceManagerState,
    delegate_compositor, delegate_keyboard, delegate_pointer, delegate_registry,
    delegate_seat, delegate_shm, delegate_xdg_shell, delegate_xdg_window, delegate_output,
    delegate_layer,
    registry::{ProvidesRegistryState, RegistryState},
    output::{OutputHandler, OutputState},
    seat::{
        keyboard::KeyboardHandler,
        pointer::{PointerHandler, ThemedPointer, ThemeSpec, CursorIcon},
        Capability, SeatHandler, SeatState,
    },
    shell::{
        xdg::{
            window::{Window as XdgWindow, WindowConfigure, WindowHandler, WindowDecorations},
            XdgShell, XdgSurface as XdgSurfaceExt,
        },
        wlr_layer::{LayerShell, LayerShellHandler, LayerSurface, LayerSurfaceConfigure},
        WaylandSurface,
    },
    shm::{Shm, ShmHandler},
};
use wayland_client::{
    globals::{registry_queue_init, GlobalList},
    protocol::{wl_keyboard, wl_output, wl_pointer, wl_seat, wl_surface, wl_registry, wl_region, wl_callback},
    Connection, QueueHandle, Proxy,
};

use wayland_protocols::wp::pointer_gestures::zv1::client::{
    zwp_pointer_gesture_pinch_v1::{self, ZwpPointerGesturePinchV1},
    zwp_pointer_gestures_v1::{self as zwp_pointer_gestures, ZwpPointerGesturesV1},
};
pub use smithay_client_toolkit::reexports::protocols::xdg::shell::client::xdg_toplevel;
pub use smithay_client_toolkit::seat::pointer::CursorIcon as PointerCursorIcon;
use calloop::EventLoop;
use calloop_wayland_source::WaylandSource;
use cosmic_text::{FontSystem, Buffer, Attrs, Metrics};
use crate::widget::{WidgetHost, TextItem, MouseButton, ElementState, MouseScrollDelta, KeyEvent, Key, NamedKey, Position};
use crate::wayland::detect_scale_factor;
use crate::vk::{Batch2D, Frame2D, TextSpan, VkRenderer};

#[derive(Hash, PartialEq, Eq, Clone)]
struct BufferCacheKey {
    text: String,
    size_milli: u32,
    font: Option<String>,
    is_vertical: bool,
    attrs: crate::scene::paint::TextAttrs,
}

#[derive(Clone)]
struct CachedBuffer {
    buffer: Buffer,
    last_accessed: std::time::Instant,
}

std::thread_local! {
    static BUFFER_CACHE: std::cell::RefCell<std::collections::HashMap<BufferCacheKey, CachedBuffer>> = std::cell::RefCell::new(std::collections::HashMap::new());
}

/// A droplet spec resolved against a concrete rect: the push-constant fields
/// that define its SILHOUETTE, in logical px.
///
/// Shared by [`crate::scene::paint::Prim::Droplet`] and
/// [`crate::scene::paint::Prim::DropletScrim`] so the lit drop and the vignette
/// drawn inside it can never disagree about the shape — the whole reason the
/// scrim rides the droplet's shader path instead of approximating the outline
/// with a rounded rect.
struct DropletGeom {
    hx: f32,
    hy: f32,
    sag: f32,
    br: f32,
    bw: f32,
    k: f32,
    sr: f32,
    ar: f32,
    band: f32,
    bow: f32,
    /// How far the contact shadow reaches below/beside the box (0 when the
    /// spec has no shadow). The lit drop's cover quad grows by this; a scrim
    /// never draws outside the silhouette and ignores it.
    sh_reach: f32,
}

fn droplet_geom(rect: &crate::scene::layout::Rect, spec: &crate::scene::paint::DropletSpec) -> DropletGeom {
    let hx = rect.width * 0.5;
    let hy = rect.height * 0.5;
    let sag = spec.sag.clamp(0.0, 0.9) * rect.height;
    // belly <= 0 disables the belly outright (the oval-dewdrop default) — the
    // shader skips the smin when the radius is 0.
    let (br, bw) = if spec.belly > 0.0 {
        let br = (spec.belly.min(1.0) * rect.height).min(hy).min(hx);
        (br, ((hx - br).max(0.0) * spec.belly_w.clamp(0.0, 1.0)).max(1.0))
    } else {
        (0.0, 0.0)
    };
    let k = (spec.blend.max(0.0) * rect.height).max(1.0);
    let sheet_hy = hy - sag * 0.5;
    // Bottom (sheet_r) and top (attach) corner radii: when the pair overfills
    // the sheet height, scale both down proportionally — 0.5 + 0.5 is the
    // fully continuous egg.
    let mut sr = (spec.sheet_r.clamp(0.0, 1.0) * rect.height).min(hx);
    let mut ar = (spec.attach.clamp(0.0, 1.0) * rect.height).min(hx);
    let sheet_h = (2.0 * sheet_hy).max(0.0);
    if sr + ar > sheet_h && sr + ar > 0.0 {
        let f = sheet_h / (sr + ar);
        sr *= f;
        ar *= f;
    }
    let band = (spec.band.max(0.05) * rect.height).max(1.0);
    // Bottom-bow edge rise; the shader derives the arc radius from it per drop
    // (R = hx^2/2*rise).
    let bow = (spec.bow.clamp(0.0, 0.5) * rect.height).min(hy * 0.9);
    let sh_reach = if spec.shadow > 0.0 { (0.18 * rect.height).max(2.0) } else { 0.0 };
    DropletGeom { hx, hy, sag, br, bw, k, sr, ar, band, bow, sh_reach }
}

fn find_cased_family(fs: &FontSystem, name: &str) -> Option<String> {
    let lower_name = name.to_lowercase();
    for face in fs.db().faces() {
        for (family, _) in &face.families {
            if family.to_lowercase() == lower_name {
                return Some(family.clone());
            }
        }
    }
    None
}

thread_local! {
    /// Family name → is-monospaced, resolved once per family from fontdb's
    /// face metadata (the post table's isFixedPitch, as fontdb records it).
    static MONO_FAMILY_CACHE: std::cell::RefCell<std::collections::HashMap<String, bool>> =
        std::cell::RefCell::new(std::collections::HashMap::new());
}

fn family_is_monospaced(fs: &FontSystem, name: &str) -> bool {
    MONO_FAMILY_CACHE.with(|cache| {
        if let Some(&mono) = cache.borrow().get(name) {
            return mono;
        }
        let lower = name.to_lowercase();
        let mono = fs
            .db()
            .faces()
            .find(|face| face.families.iter().any(|(f, _)| f.to_lowercase() == lower))
            .map(|face| face.monospaced)
            .unwrap_or(false);
        cache.borrow_mut().insert(name.to_string(), mono);
        mono
    })
}

/// The shaping mode for one text run: ASCII-only text in a MONOSPACED face
/// shapes `Basic`, everything else `Advanced`.
///
/// `Basic` bypasses OpenType substitution and positioning, and for ASCII in a
/// mono face that is exactly right: a mono font's ligatures are the one thing
/// `Advanced` adds there, and they break the grid — Chivo Mono's `liga`
/// squeezes f+i into a single-advance ﬁ glyph, which is why the bar's window
/// titles rendered "file" with a cramped fi — while mono faces carry no
/// kerning to lose. Proportional faces keep `Advanced` (their kerning and
/// ligatures are wanted — a font preview must not misrepresent the face), and
/// any non-ASCII text keeps real shaping (combining marks, emoji, complex
/// scripts) whatever the face.
pub fn shaping_for(fs: &FontSystem, text: &str, family: &cosmic_text::Family) -> cosmic_text::Shaping {
    if text.is_ascii() {
        if let cosmic_text::Family::Name(name) = family {
            if family_is_monospaced(fs, name) {
                return cosmic_text::Shaping::Basic;
            }
        }
    }
    cosmic_text::Shaping::Advanced
}

pub fn get_text_buffer(fs: &mut FontSystem, text: &str, size: f32, font: Option<&str>) -> Buffer {
    get_text_buffer_attrs(fs, text, size, font, crate::scene::paint::TextAttrs::default())
}

/// [`get_text_buffer`] plus shaping attributes (italic / weight) — the backend's shape entry
/// for `Prim::Text` prims that carry [`TextAttrs`] (the font picker's style-variant previews).
pub fn get_text_buffer_attrs(
    fs: &mut FontSystem,
    text: &str,
    size: f32,
    font: Option<&str>,
    text_attrs: crate::scene::paint::TextAttrs,
) -> Buffer {
    let scale = crate::scale::scale_factor();
    let mut font_size = size;
    let mut family_name = None;

    if let Some(font_str) = font {
        let (parsed_family, parsed_size) = crate::layout::parse_font_string(font_str);
        if let Some(ps) = parsed_size {
            font_size = ps;
        }
        family_name = Some(parsed_family);
    }

    let physical_size = font_size * scale;
    let size_key = (physical_size * 1000.0).round() as u32;
    let is_vertical = crate::IS_VERTICAL.load(std::sync::atomic::Ordering::Relaxed);
    let key = BufferCacheKey {
        text: text.to_string(),
        size_milli: size_key,
        font: family_name.clone(),
        is_vertical,
        attrs: text_attrs,
    };

    let cached = BUFFER_CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        if let Some(cached_item) = cache.get_mut(&key) {
            cached_item.last_accessed = std::time::Instant::now();
            Some(cached_item.buffer.clone())
        } else {
            None
        }
    });

    if let Some(buf) = cached {
        return buf;
    }

    let line_height = if is_vertical {
        physical_size * 1.05
    } else {
        physical_size * 1.0
    };
    let metrics = Metrics::new(physical_size, line_height);
    let mut buf = Buffer::new(fs, metrics);
    let mut attrs = Attrs::new();

    let (sans_fallback, serif_fallback, mono_fallback, _) = crate::layout::read_preferred_fonts();

    let resolved_storage = family_name.as_deref().and_then(|font_name| match font_name {
        "monospace" if !mono_fallback.is_empty() => find_cased_family(fs, &mono_fallback),
        "sans-serif" if !sans_fallback.is_empty() => find_cased_family(fs, &sans_fallback),
        "serif" if !serif_fallback.is_empty() => find_cased_family(fs, &serif_fallback),
        _ => None,
    });

    let resolved_sans = if !sans_fallback.is_empty() {
        find_cased_family(fs, &sans_fallback)
    } else {
        None
    };

    let family = if let Some(ref font_family) = family_name {
        match font_family.as_str() {
            "monospace" => {
                if !mono_fallback.is_empty() {
                    if let Some(ref cased) = resolved_storage {
                        cosmic_text::Family::Name(cased)
                    } else {
                        cosmic_text::Family::Name(crate::layout::get_system_monospace_font())
                    }
                } else {
                    cosmic_text::Family::Name(crate::layout::get_system_monospace_font())
                }
            }
            "sans-serif" => {
                if !sans_fallback.is_empty() {
                    if let Some(ref cased) = resolved_storage {
                        cosmic_text::Family::Name(cased)
                    } else {
                        cosmic_text::Family::SansSerif
                    }
                } else {
                    cosmic_text::Family::SansSerif
                }
            }
            "serif" => {
                if !serif_fallback.is_empty() {
                    if let Some(ref cased) = resolved_storage {
                        cosmic_text::Family::Name(cased)
                    } else {
                        cosmic_text::Family::Serif
                    }
                } else {
                    cosmic_text::Family::Serif
                }
            }
            name => cosmic_text::Family::Name(name),
        }
    } else {
        if !sans_fallback.is_empty() {
            if let Some(ref cased) = resolved_sans {
                cosmic_text::Family::Name(cased)
            } else {
                cosmic_text::Family::SansSerif
            }
        } else {
            cosmic_text::Family::SansSerif
        }
    };
    attrs = attrs.family(family);
    if text_attrs.italic {
        attrs = attrs.style(cosmic_text::Style::Italic);
    }
    if let Some(w) = text_attrs.weight {
        attrs = attrs.weight(cosmic_text::Weight(w));
    }
    let shaping = shaping_for(fs, text, &family);
    buf.set_text(fs, text, attrs, shaping);
    buf.shape_until_scroll(fs, true);

    BUFFER_CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        if cache.len() >= 2000 {
            let mut items: Vec<(BufferCacheKey, std::time::Instant)> = cache
                .iter()
                .map(|(k, v)| (k.clone(), v.last_accessed))
                .collect();
            items.sort_by_key(|&(_, time)| time);
            for (k, _) in items.iter().take(100) {
                cache.remove(k);
            }
        }
        cache.insert(key, CachedBuffer {
            buffer: buf.clone(),
            last_accessed: std::time::Instant::now(),
        });
    });

    buf
}

/// Byte-offset → x mapping of single-line `text`, shaped exactly as the renderer draws it —
/// same buffer cache as the draw, so this is a lookup when the text is already on screen.
/// Returns ascending `(byte_idx, x)` pairs (one per cluster start, logical px, relative to
/// the text origin), terminated by `(text.len(), total_advance)`. This is the correct
/// source for caret placement and click→cursor mapping in hand-rolled text fields:
/// `measure_text_width` reports SVG-rasterized inked extent through fontdb's family
/// resolution, which disagrees with cosmic-text's advance and can even resolve a
/// different face — a caret placed with it drifts off the drawn glyphs.
pub fn shaped_cluster_offsets(
    fs: &mut FontSystem,
    text: &str,
    size: f32,
    font: Option<&str>,
) -> Vec<(usize, f32)> {
    let scale = crate::scale::scale_factor().max(1.0);
    let buffer = get_text_buffer(fs, text, size, font);
    let mut out: Vec<(usize, f32)> = Vec::new();
    let mut total: f32 = 0.0;
    for (start, x, w) in normalized_glyph_starts(&buffer, text) {
        if out.last().map_or(true, |&(b, _)| b != start) {
            out.push((start, x / scale));
        }
        total = total.max((x + w) / scale);
    }
    out.push((text.len(), total));
    out
}

/// Every glyph of `buffer`'s layout runs as `(start_byte, x, w)` (physical px),
/// with `start` normalized to be text-relative.
///
/// Exists because cosmic-text 0.12's `Shaping::Basic` path (`shape_skip`) emits
/// `LayoutGlyph::start` relative to the shape SPAN — it resets to 0 at every
/// word — while the Advanced path emits line-relative starts. `shaping_for`
/// picks Basic exactly for ASCII text in a monospace family (the DE's default
/// control font), so any multi-word value hit the bug: offsets keyed by those
/// starts collide on the low columns and the caret/selection walk off the
/// glyphs. A reset can ONLY come from that path, which shapes strictly one
/// glyph per char in logical order — so when one is seen, byte starts are
/// rebuilt by walking the text's chars. `text` must be the single line the
/// buffer was shaped from.
pub(crate) fn normalized_glyph_starts(buffer: &Buffer, text: &str) -> Vec<(usize, f32, f32)> {
    let mut glyphs: Vec<(usize, f32, f32)> = Vec::new();
    let mut monotonic = true;
    let mut prev = 0usize;
    for run in buffer.layout_runs() {
        for g in run.glyphs {
            if g.start < prev {
                monotonic = false;
            }
            prev = g.start;
            glyphs.push((g.start, g.x, g.w));
        }
    }
    if !monotonic {
        let mut starts = text.char_indices().map(|(i, _)| i);
        for g in glyphs.iter_mut() {
            g.0 = starts.next().unwrap_or(text.len());
        }
    }
    glyphs
}

/// Shape a boxed [`Prim::Text`] (word-wrap + alignment) and return `(buffer, vertical_offset)`.
/// Reuses [`get_text_buffer_attrs`] for all the family resolution — that returns a *clone* of the
/// cached single-run buffer, so re-applying metrics/size/align here does not touch the cache — then
/// re-lays-it-out: a 1.4 line-height (the placed-text convention), the wrap width, per-line
/// horizontal alignment, and re-shapes. The vertical offset positions the shaped block inside the
/// box per `align_v`. Uncached by construction (each box may differ in width/align).
pub fn get_text_buffer_laid_out(
    fs: &mut FontSystem,
    text: &str,
    size: f32,
    font: Option<&str>,
    text_attrs: crate::scene::paint::TextAttrs,
    layout: crate::scene::paint::TextLayout,
) -> (Buffer, f32) {
    use crate::scene::paint::{AlignH, AlignV};
    let scale = crate::scale::scale_factor();

    // Resolved family + attrs come for free (a cache clone we are free to mutate).
    let mut buf = get_text_buffer_attrs(fs, text, size, font, text_attrs);

    // The font string may override the size ("family:size") — mirror get_text_buffer_attrs.
    let mut font_size = size;
    if let Some(font_str) = font {
        if let (_, Some(ps)) = crate::layout::parse_font_string(font_str) {
            font_size = ps;
        }
    }
    let physical_size = font_size * scale;
    let line_height = physical_size * 1.4;
    buf.set_metrics(fs, Metrics::new(physical_size, line_height));
    buf.set_size(fs, layout.wrap_width.map(|w| w * scale), Some(layout.box_height * scale));

    let align = match layout.align_h {
        AlignH::Left => cosmic_text::Align::Left,
        AlignH::Center => cosmic_text::Align::Center,
        AlignH::Right => cosmic_text::Align::Right,
    };
    for line in &mut buf.lines {
        line.set_align(Some(align));
    }
    buf.shape_until_scroll(fs, true);

    // Vertical offset (logical) from the shaped run count, matching the legacy per-app math.
    let runs = buf.layout_runs().count();
    let total_h = runs as f32 * font_size * 1.4;
    let voff = match layout.align_v {
        AlignV::Top => 0.0,
        AlignV::Middle => ((layout.box_height - total_h) / 2.0).max(0.0),
        AlignV::Bottom => (layout.box_height - total_h).max(0.0),
    };
    (buf, voff)
}

/// A text item's clip rect in physical pixels. This was `glyphon::TextBounds` — the one
/// glyphon-owned type cce-ui ever used, everything else being a cosmic-text re-export — so
/// it is defined here now that the dependency is cosmic-text directly. Same plain
/// four-`i32` layout; it is only an intermediate on the way to `TextSpan::bounds`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TextBounds {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

/// The popover-occlusion clamp shared by the default [`Application::text_areas`] mapping and
/// the display-list text path: clip a text item's bounds so it does not bleed through an open
/// popover's plate. A text item whose own bounds coincide with a popover rect IS that popover's
/// text and is left alone; anything else that intersects gets clamped horizontally toward
/// whichever side of the popover it starts on.
fn popover_occlusion_clamp(
    overlay_rects: &[(f32, f32, f32, f32)],
    ti: &TextItem,
    scale_f32: f32,
    item_bounds: &mut TextBounds,
) {
    for &(ox, oy, ow, oh) in overlay_rects {
        let ol = (ox * scale_f32).round() as i32;
        let ot = (oy * scale_f32).round() as i32;
        let or = ((ox + ow) * scale_f32).round() as i32;
        let ob = ((oy + oh) * scale_f32).round() as i32;

        let is_overlay_text = if let Some([l, t, r, b]) = ti.bounds {
            (l - ox).abs() < 1.0
                && (t - oy).abs() < 1.0
                && (r - (ox + ow)).abs() < 1.0
                && (b - (oy + oh)).abs() < 1.0
        } else {
            false
        };

        if !is_overlay_text {
            let tx_pixel = ti.x * scale_f32;
            let ty_pixel = ti.y * scale_f32;

            let mut text_w = 0.0f32;
            let mut run_count = 0;
            for run in ti.buffer.layout_runs() {
                text_w = text_w.max(run.line_w);
                run_count += 1;
            }
            let text_h = run_count as f32 * ti.buffer.metrics().line_height;

            let actual_left = tx_pixel;
            let actual_right = tx_pixel + text_w;
            let actual_top = ty_pixel;
            let actual_bottom = ty_pixel + text_h;

            if actual_left < or as f32
                && actual_right > ol as f32
                && actual_top < ob as f32
                && actual_bottom > ot as f32
            {
                if tx_pixel < ol as f32 {
                    item_bounds.right = item_bounds.right.min(ol);
                } else {
                    item_bounds.left = item_bounds.left.max(or);
                }
            }
        }
    }
}

#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Vertex {
    pub position: [f32; 2],
    pub color: [f32; 4],
    pub clip_circle: [f32; 3], // [cx, cy, r]
}

pub fn quad_vertices(x: f32, y: f32, w: f32, h: f32, sw: f32, sh: f32, c: [f32; 4]) -> [Vertex; 6] {
    let x0 = (x / sw) * 2.0 - 1.0;
    let y0 = 1.0 - (y / sh) * 2.0;
    let x1 = ((x + w) / sw) * 2.0 - 1.0;
    let y1 = 1.0 - ((y + h) / sh) * 2.0;
    [
        Vertex { position: [x0, y0], color: c, clip_circle: [0.0, 0.0, 0.0] },
        Vertex { position: [x1, y0], color: c, clip_circle: [0.0, 0.0, 0.0] },
        Vertex { position: [x0, y1], color: c, clip_circle: [0.0, 0.0, 0.0] },
        Vertex { position: [x1, y0], color: c, clip_circle: [0.0, 0.0, 0.0] },
        Vertex { position: [x1, y1], color: c, clip_circle: [0.0, 0.0, 0.0] },
        Vertex { position: [x0, y1], color: c, clip_circle: [0.0, 0.0, 0.0] },
    ]
}

pub fn quad_vertices_with_clip(
    x: f32, y: f32, w: f32, h: f32,
    sw: f32, sh: f32,
    color: [f32; 4],
    clip_circle: [f32; 3],
) -> [Vertex; 6] {
    let x0 = (x / sw) * 2.0 - 1.0;
    let y0 = 1.0 - (y / sh) * 2.0;
    let x1 = ((x + w) / sw) * 2.0 - 1.0;
    let y1 = 1.0 - ((y + h) / sh) * 2.0;
    [
        Vertex { position: [x0, y0], color, clip_circle },
        Vertex { position: [x1, y0], color, clip_circle },
        Vertex { position: [x0, y1], color, clip_circle },
        Vertex { position: [x1, y0], color, clip_circle },
        Vertex { position: [x1, y1], color, clip_circle },
        Vertex { position: [x0, y1], color, clip_circle },
    ]
}

/// A quad whose four corners each carry their own color, Gouraud-interpolated across both
/// triangles by the shader (`@location(0) color` has no `flat` qualifier). Corner order is
/// TL, TR, BR, BL. Keep the alpha equal on all four: negative alpha is the blur sentinel,
/// so a gradient that crossed zero would tear the triangle in half.
pub fn quad_vertices_shaded(
    x: f32, y: f32, w: f32, h: f32,
    sw: f32, sh: f32,
    c_tl: [f32; 4], c_tr: [f32; 4], c_br: [f32; 4], c_bl: [f32; 4],
    clip_circle: [f32; 3],
) -> [Vertex; 6] {
    let x0 = (x / sw) * 2.0 - 1.0;
    let y0 = 1.0 - (y / sh) * 2.0;
    let x1 = ((x + w) / sw) * 2.0 - 1.0;
    let y1 = 1.0 - ((y + h) / sh) * 2.0;
    [
        Vertex { position: [x0, y0], color: c_tl, clip_circle },
        Vertex { position: [x1, y0], color: c_tr, clip_circle },
        Vertex { position: [x0, y1], color: c_bl, clip_circle },
        Vertex { position: [x1, y0], color: c_tr, clip_circle },
        Vertex { position: [x1, y1], color: c_br, clip_circle },
        Vertex { position: [x0, y1], color: c_bl, clip_circle },
    ]
}

pub fn quad_vertices_clipped(
    x: f32, y: f32, w: f32, h: f32,
    surface_w: f32, surface_h: f32,
    color: [f32; 4],
    clip: (f32, f32, f32, f32),
    clip_circle: [f32; 3],
) -> Vec<Vertex> {
    let (cx0, cy0, cx1, cy1) = clip;
    let ix0 = x.max(cx0);
    let iy0 = y.max(cy0);
    let ix1 = (x + w).min(cx1);
    let iy1 = (y + h).min(cy1);
    if ix1 <= ix0 || iy1 <= iy0 {
        return Vec::new();
    }
    quad_vertices_with_clip(ix0, iy0, ix1 - ix0, iy1 - iy0, surface_w, surface_h, color, clip_circle).to_vec()
}

pub fn line_vertices(
    x1: f32, y1: f32, x2: f32, y2: f32,
    thickness: f32,
    sw: f32, sh: f32,
    c: [f32; 4]
) -> [Vertex; 6] {
    let dx = x2 - x1;
    let dy = y2 - y1;
    let len = (dx * dx + dy * dy).sqrt();
    if len < 0.001 {
        return quad_vertices(x1 - thickness/2.0, y1 - thickness/2.0, thickness, thickness, sw, sh, c);
    }
    let ux = dx / len;
    let uy = dy / len;
    let nx = -uy;
    let ny = ux;
    
    let half_t = thickness * 0.5;
    let p0x = x1 + nx * half_t;
    let p0y = y1 + ny * half_t;
    let p1x = x1 - nx * half_t;
    let p1y = y1 - ny * half_t;
    let p2x = x2 - nx * half_t;
    let p2y = y2 - ny * half_t;
    let p3x = x2 + nx * half_t;
    let p3y = y2 + ny * half_t;

    let ndc_p0x = (p0x / sw) * 2.0 - 1.0;
    let ndc_p0y = 1.0 - (p0y / sh) * 2.0;
    let ndc_p1x = (p1x / sw) * 2.0 - 1.0;
    let ndc_p1y = 1.0 - (p1y / sh) * 2.0;
    let ndc_p2x = (p2x / sw) * 2.0 - 1.0;
    let ndc_p2y = 1.0 - (p2y / sh) * 2.0;
    let ndc_p3x = (p3x / sw) * 2.0 - 1.0;
    let ndc_p3y = 1.0 - (p3y / sh) * 2.0;

    let clip_circle = [0.0, 0.0, 0.0];
    [
        Vertex { position: [ndc_p0x, ndc_p0y], color: c, clip_circle },
        Vertex { position: [ndc_p1x, ndc_p1y], color: c, clip_circle },
        Vertex { position: [ndc_p2x, ndc_p2y], color: c, clip_circle },
        Vertex { position: [ndc_p0x, ndc_p0y], color: c, clip_circle },
        Vertex { position: [ndc_p2x, ndc_p2y], color: c, clip_circle },
        Vertex { position: [ndc_p3x, ndc_p3y], color: c, clip_circle },
    ]
}

#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum LineCap {
    Arrow,
    Round,
    Flat,
}

pub fn vector_vertices(
    x1: f32, y1: f32, x2: f32, y2: f32,
    thickness: f32,
    sw: f32, sh: f32,
    c: [f32; 4],
    line_cap: LineCap,
) -> Vec<Vertex> {
    let mut verts = Vec::new();
    let dx = x2 - x1;
    let dy = y2 - y1;
    let len = (dx * dx + dy * dy).sqrt();
    if len < 0.001 {
        return quad_vertices(x1 - thickness/2.0, y1 - thickness/2.0, thickness, thickness, sw, sh, c).to_vec();
    }
    
    match line_cap {
        LineCap::Arrow => {
            let ux = dx / len;
            let uy = dy / len;
            let nx = -uy;
            let ny = ux;
            
            let arrow_len = (thickness * 3.0).max(10.0).min(len);
            let arrow_width = (thickness * 2.5).max(8.0);
            
            let line_x2 = x2 - ux * arrow_len;
            let line_y2 = y2 - uy * arrow_len;
            
            if len > arrow_len {
                verts.extend_from_slice(&line_vertices(x1, y1, line_x2, line_y2, thickness, sw, sh, c));
            }
            
            let bx = line_x2;
            let by = line_y2;
            
            let w1x = bx + nx * (arrow_width * 0.5);
            let w1y = by + ny * (arrow_width * 0.5);
            let w2x = bx - nx * (arrow_width * 0.5);
            let w2y = by - ny * (arrow_width * 0.5);
            
            let ndc_tip_x = (x2 / sw) * 2.0 - 1.0;
            let ndc_tip_y = 1.0 - (y2 / sh) * 2.0;
            let ndc_w1x = (w1x / sw) * 2.0 - 1.0;
            let ndc_w1y = 1.0 - (w1y / sh) * 2.0;
            let ndc_w2x = (w2x / sw) * 2.0 - 1.0;
            let ndc_w2y = 1.0 - (w2y / sh) * 2.0;
            
            let clip_circle = [0.0, 0.0, 0.0];
            verts.push(Vertex { position: [ndc_tip_x, ndc_tip_y], color: c, clip_circle });
            verts.push(Vertex { position: [ndc_w1x, ndc_w1y], color: c, clip_circle });
            verts.push(Vertex { position: [ndc_w2x, ndc_w2y], color: c, clip_circle });
        }
        LineCap::Round => {
            push_feathered_line_vertices(x1, y1, x2, y2, thickness, sw, sh, c, &mut verts);
            let clip_circle = [0.0, 0.0, 0.0];
            verts.extend(circle_vertices(x2, y2, thickness / 2.0, sw, sh, c, 16, clip_circle));
        }
        LineCap::Flat => {
            push_feathered_line_vertices(x1, y1, x2, y2, thickness, sw, sh, c, &mut verts);
        }
    }

    verts
}

/// `line_vertices` with a half-px alpha ramp along each long edge (the arc
/// tessellator's poor-man's AA) — diagonal strokes resolve smoothly instead of
/// stair-stepping. Axis-aligned strokes keep the crisp single-quad path:
/// feathering a pixel-snapped hairline would only blur it.
fn push_feathered_line_vertices(
    x1: f32, y1: f32, x2: f32, y2: f32,
    thickness: f32,
    sw: f32, sh: f32,
    c: [f32; 4],
    out: &mut Vec<Vertex>,
) {
    let dx = x2 - x1;
    let dy = y2 - y1;
    let len = (dx * dx + dy * dy).sqrt();
    if len < 0.001 || dx.abs() < 0.01 || dy.abs() < 0.01 {
        out.extend_from_slice(&line_vertices(x1, y1, x2, y2, thickness, sw, sh, c));
        return;
    }
    let (nx, ny) = (-dy / len, dx / len);
    let f = 0.5f32.min(thickness * 0.25);
    let half = thickness * 0.5;
    // (offset at band start, offset at band end, alpha at start, alpha at end)
    let bands = [
        (-half - f, -half + f, 0.0, c[3]),
        (-half + f, half - f, c[3], c[3]),
        (half - f, half + f, c[3], 0.0),
    ];
    for &(oa, ob, aa, ab) in &bands {
        let ca = [c[0], c[1], c[2], aa];
        let cb = [c[0], c[1], c[2], ab];
        let p = |x: f32, y: f32, o: f32| -> [f32; 2] {
            [((x + nx * o) / sw) * 2.0 - 1.0, 1.0 - ((y + ny * o) / sh) * 2.0]
        };
        let clip_circle = [0.0, 0.0, 0.0];
        let (a1, b1) = (p(x1, y1, oa), p(x1, y1, ob));
        let (a2, b2) = (p(x2, y2, oa), p(x2, y2, ob));
        out.push(Vertex { position: a1, color: ca, clip_circle });
        out.push(Vertex { position: b1, color: cb, clip_circle });
        out.push(Vertex { position: b2, color: cb, clip_circle });
        out.push(Vertex { position: a1, color: ca, clip_circle });
        out.push(Vertex { position: b2, color: cb, clip_circle });
        out.push(Vertex { position: a2, color: ca, clip_circle });
    }
}

pub fn rounded_rect_vertices_corners(
    x: f32, y: f32, ww: f32, h: f32,
    r: f32,
    sw: f32, sh: f32,
    color: [f32; 4],
    clip_circle: [f32; 3],
    corners: (bool, bool, bool, bool),
    clip_rect: Option<(f32, f32, f32, f32)>,
) -> Vec<Vertex> {
    let mut verts = Vec::new();
    let radii = crate::widget::CornerRadii::new(
        if corners.0 { r } else { 0.0 },
        if corners.1 { r } else { 0.0 },
        if corners.2 { r } else { 0.0 },
        if corners.3 { r } else { 0.0 },
    );
    push_rounded_rect_vertices_corners(x, y, ww, h, radii, sw, sh, color, clip_circle, clip_rect, &mut verts);
    verts
}

/// Sample of the unit superellipse |x|^n + |y|^n = 1 at circle parameter θ —
/// the (cos θ, sin θ) replacement the corner fans use. Exactly the circle at
/// n = 2; higher `corner_shape` exponents give the DE's continuous-curvature
/// corners, so widget silhouettes follow the same corner family as the
/// SDF-lit plates. `e` is 2/n, hoisted by callers. Tangent points at the
/// quadrant ends are unchanged, so fans still tile exactly against the body
/// rects and edge strips.
#[inline]
fn superellipse_pt(theta: f32, e: f32) -> (f32, f32) {
    let (s, c) = theta.sin_cos();
    (c.signum() * c.abs().powf(e), s.signum() * s.abs().powf(e))
}

/// Feathered glow ([`Prim::Glow`]): the rounded rect's interior fills at the
/// color's alpha and concentric outline rings fade it to zero across `reach`
/// px outside the boundary. Alpha rides the VERTICES, so the GPU interpolates
/// a per-pixel-smooth falloff between rings — stacked translucent layers band
/// visibly; this cannot. Ring alphas sit on a quadratic ease-out, giving the
/// vignette profile piecewise-linearly with kinks below visibility at glow
/// alphas. Corners sample [`superellipse_pt`], so a glow's silhouette sits in
/// the same corner family as the cells, nodes, and plates it highlights.
pub fn push_glow_vertices(
    x: f32, y: f32, ww: f32, h: f32,
    radius: f32, reach: f32,
    sw: f32, sh: f32,
    color: [f32; 4],
    clip_circle: [f32; 3],
    out: &mut Vec<Vertex>,
) {
    if ww <= 0.0 || h <= 0.0 || color[3].abs() <= 0.0005 || sw <= 0.0 || sh <= 0.0 {
        return;
    }
    let r0 = radius.clamp(0.0, ww.min(h) * 0.5);
    let ctl = (x + r0, y + r0);
    let ctr = (x + ww - r0, y + r0);
    let cbr = (x + ww - r0, y + h - r0);
    let cbl = (x + r0, y + h - r0);
    const K: usize = 10;
    use std::f32::consts::PI;
    let corner_e = 2.0 / crate::layout::corner_shape();
    // One outline ring `off` px outside the boundary, clockwise from the
    // top-left arc; every ring shares the layout, so strips never twist.
    let ring = |off: f32| -> Vec<[f32; 2]> {
        let r = (r0 + off).max(0.0);
        let mut pts = Vec::with_capacity(4 * (K + 1));
        let corners = [
            (ctl, PI, 1.5 * PI),
            (ctr, 1.5 * PI, 2.0 * PI),
            (cbr, 0.0, 0.5 * PI),
            (cbl, 0.5 * PI, PI),
        ];
        for ((cx, cy), a0, a1) in corners {
            for k in 0..=K {
                let a = a0 + (a1 - a0) * (k as f32 / K as f32);
                let (ux, uy) = superellipse_pt(a, corner_e);
                pts.push([cx + r * ux, cy + r * uy]);
            }
        }
        pts
    };
    let to_v = |p: [f32; 2], a: f32| Vertex {
        position: [(p[0] / sw) * 2.0 - 1.0, 1.0 - (p[1] / sh) * 2.0],
        color: [color[0], color[1], color[2], a],
        clip_circle,
    };

    let rings: Vec<(Vec<[f32; 2]>, f32)> = [0.0f32, 0.35, 0.7, 1.0]
        .iter()
        .map(|&t| (ring(reach * t), color[3] * (1.0 - t) * (1.0 - t)))
        .collect();
    let n = rings[0].0.len();

    // Interior: a fan from the rect center over the innermost ring (a rounded
    // rect is convex, so the fan covers it exactly), uniform core alpha.
    let center = [x + ww * 0.5, y + h * 0.5];
    for i in 0..n {
        let p1 = rings[0].0[i];
        let p2 = rings[0].0[(i + 1) % n];
        out.push(to_v(center, color[3]));
        out.push(to_v(p1, color[3]));
        out.push(to_v(p2, color[3]));
    }
    // The feather: strips between consecutive rings, each vertex carrying its
    // ring's alpha.
    for w in rings.windows(2) {
        let (inner, ia) = (&w[0].0, w[0].1);
        let (outer, oa) = (&w[1].0, w[1].1);
        for i in 0..n {
            let a1 = inner[i];
            let a2 = inner[(i + 1) % n];
            let b1 = outer[i];
            let b2 = outer[(i + 1) % n];
            out.push(to_v(a1, ia));
            out.push(to_v(b1, oa));
            out.push(to_v(a2, ia));
            out.push(to_v(a2, ia));
            out.push(to_v(b1, oa));
            out.push(to_v(b2, oa));
        }
    }
}

pub fn push_rounded_rect_vertices_corners(
    x: f32, y: f32, ww: f32, h: f32,
    radii: crate::widget::CornerRadii,
    sw: f32, sh: f32,
    color: [f32; 4],
    clip_circle: [f32; 3],
    clip_rect: Option<(f32, f32, f32, f32)>,
    out: &mut Vec<Vertex>,
) {
    let corner_e = 2.0 / crate::layout::corner_shape();
    let mut r_tl = radii.top_left.max(0.0);
    let mut r_tr = radii.top_right.max(0.0);
    let mut r_br = radii.bottom_right.max(0.0);
    let mut r_bl = radii.bottom_left.max(0.0);

    // Simple scale clamping
    let sum_top = r_tl + r_tr;
    if sum_top > ww {
        let f = ww / sum_top;
        r_tl *= f;
        r_tr *= f;
    }
    let sum_bottom = r_bl + r_br;
    if sum_bottom > ww {
        let f = ww / sum_bottom;
        r_bl *= f;
        r_br *= f;
    }
    let sum_left = r_tl + r_bl;
    if sum_left > h {
        let f = h / sum_left;
        r_tl *= f;
        r_bl *= f;
    }
    let sum_right = r_tr + r_br;
    if sum_right > h {
        let f = h / sum_right;
        r_tr *= f;
        r_br *= f;
    }

    let clamp_x = |val: f32| -> f32 {
        if let Some((cx0, _, cx1, _)) = clip_rect {
            val.max(cx0).min(cx1)
        } else {
            val
        }
    };
    let clamp_y = |val: f32| -> f32 {
        if let Some((_, cy0, _, cy1)) = clip_rect {
            val.max(cy0).min(cy1)
        } else {
            val
        }
    };

    let push_quad = |verts: &mut Vec<Vertex>, qx: f32, qy: f32, qw: f32, qh: f32| {
        let x0 = clamp_x(qx);
        let y0 = clamp_y(qy);
        let x1 = clamp_x(qx + qw);
        let y1 = clamp_y(qy + qh);
        
        if x1 <= x0 || y1 <= y0 {
            return;
        }

        let ndc_x0 = (x0 / sw) * 2.0 - 1.0;
        let ndc_y0 = 1.0 - (y0 / sh) * 2.0;
        let ndc_x1 = (x1 / sw) * 2.0 - 1.0;
        let ndc_y1 = 1.0 - (y1 / sh) * 2.0;
        
        verts.push(Vertex { position: [ndc_x0, ndc_y0], color, clip_circle });
        verts.push(Vertex { position: [ndc_x1, ndc_y0], color, clip_circle });
        verts.push(Vertex { position: [ndc_x0, ndc_y1], color, clip_circle });
        verts.push(Vertex { position: [ndc_x1, ndc_y0], color, clip_circle });
        verts.push(Vertex { position: [ndc_x1, ndc_y1], color, clip_circle });
        verts.push(Vertex { position: [ndc_x0, ndc_y1], color, clip_circle });
    };

    let has_corners = r_tl > 0.1 || r_tr > 0.1 || r_br > 0.1 || r_bl > 0.1;
    if !has_corners {
        push_quad(out, x, y, ww, h);
        return;
    }

    // Body rectangles
    let mid_x0 = r_tl.max(r_bl);
    let mid_x1 = ww - r_tr.max(r_br);
    if mid_x1 > mid_x0 {
        push_quad(out, x + mid_x0, y, mid_x1 - mid_x0, h);
    }
    if h > r_tl + r_bl {
        push_quad(out, x, y + r_tl, mid_x0, h - r_tl - r_bl);
    }
    if h > r_tr + r_br {
        push_quad(out, x + mid_x1, y + r_tr, ww - mid_x1, h - r_tr - r_br);
    }

    // Corner rendering. The fans are FEATHERED: the fan body stops half a
    // pixel short of the silhouette and a strip fades from opaque at
    // silhouette-0.5 to transparent at silhouette+0.5, so the arc
    // anti-aliases instead of rasterizing a hard staircase — invisible on
    // HiDPI widget buffers, glaring on the desktop grid's world-scale
    // cells. Perceived size is unchanged (the 50%-coverage line stays on
    // the exact silhouette). Radii too small to feather keep the hard fan.
    let segments = 16;
    let fade = [color[0], color[1], color[2], 0.0];
    let to_ndc = |px: f32, py: f32| -> [f32; 2] {
        [(px / sw) * 2.0 - 1.0, 1.0 - (py / sh) * 2.0]
    };
    let push_corner = |out: &mut Vec<Vertex>, cx: f32, cy: f32, r: f32, start: f32, end: f32| {
        let feather = r > 1.5;
        let r_fan = if feather { r - 0.5 } else { r };
        let r_out = r + 0.5;
        for i in 0..segments {
            let theta1 = start + (i as f32) * (end - start) / (segments as f32);
            let theta2 = start + ((i + 1) as f32) * (end - start) / (segments as f32);

            let (c1, s1) = superellipse_pt(theta1, corner_e);
            let (c2, s2) = superellipse_pt(theta2, corner_e);
            let p0 = to_ndc(clamp_x(cx), clamp_y(cy));
            let p1 = to_ndc(clamp_x(cx + r_fan * c1), clamp_y(cy + r_fan * s1));
            let p2 = to_ndc(clamp_x(cx + r_fan * c2), clamp_y(cy + r_fan * s2));

            out.push(Vertex { position: p0, color, clip_circle });
            out.push(Vertex { position: p1, color, clip_circle });
            out.push(Vertex { position: p2, color, clip_circle });

            if feather {
                let q1 = to_ndc(clamp_x(cx + r_out * c1), clamp_y(cy + r_out * s1));
                let q2 = to_ndc(clamp_x(cx + r_out * c2), clamp_y(cy + r_out * s2));
                out.push(Vertex { position: p1, color, clip_circle });
                out.push(Vertex { position: q1, color: fade, clip_circle });
                out.push(Vertex { position: q2, color: fade, clip_circle });
                out.push(Vertex { position: p1, color, clip_circle });
                out.push(Vertex { position: q2, color: fade, clip_circle });
                out.push(Vertex { position: p2, color, clip_circle });
            }
        }
    };

    // Top-Left
    if r_tl > 0.1 {
        push_corner(out, x + r_tl, y + r_tl, r_tl, std::f32::consts::PI, 1.5 * std::f32::consts::PI);
        if mid_x0 > r_tl {
            push_quad(out, x + r_tl, y, mid_x0 - r_tl, r_tl);
        }
    }

    // Top-Right
    if r_tr > 0.1 {
        push_corner(out, x + ww - r_tr, y + r_tr, r_tr, 1.5 * std::f32::consts::PI, 2.0 * std::f32::consts::PI);
        if ww - mid_x1 > r_tr {
            push_quad(out, x + mid_x1, y, ww - mid_x1 - r_tr, r_tr);
        }
    }

    // Bottom-Right
    if r_br > 0.1 {
        push_corner(out, x + ww - r_br, y + h - r_br, r_br, 0.0, 0.5 * std::f32::consts::PI);
        if ww - mid_x1 > r_br {
            push_quad(out, x + mid_x1, y + h - r_br, ww - mid_x1 - r_br, r_br);
        }
    }

    // Bottom-Left
    if r_bl > 0.1 {
        push_corner(out, x + r_bl, y + h - r_bl, r_bl, 0.5 * std::f32::consts::PI, std::f32::consts::PI);
        if mid_x0 > r_bl {
            push_quad(out, x + r_bl, y + h - r_bl, mid_x0 - r_bl, r_bl);
        }
    }
}

pub fn rounded_rect_vertices(
    x: f32, y: f32, ww: f32, h: f32,
    r: f32,
    sw: f32, sh: f32,
    color: [f32; 4],
    clip_circle: [f32; 3],
) -> Vec<Vertex> {
    let mut verts = Vec::new();
    push_rounded_rect_vertices_corners(x, y, ww, h, crate::widget::CornerRadii::uniform(r), sw, sh, color, clip_circle, None, &mut verts);
    verts
}

pub fn push_rounded_rect_vertices(
    x: f32, y: f32, ww: f32, h: f32,
    r: f32,
    sw: f32, sh: f32,
    color: [f32; 4],
    clip_circle: [f32; 3],
    out: &mut Vec<Vertex>,
) {
    push_rounded_rect_vertices_corners(x, y, ww, h, crate::widget::CornerRadii::uniform(r), sw, sh, color, clip_circle, None, out);
}

pub fn plate_bevel_vertices(
    x: f32, y: f32, ww: f32, h: f32,
    r: f32,
    t: f32,
    sw: f32, sh: f32,
    base_color: [f32; 4],
    clip_circle: [f32; 3],
) -> Vec<Vertex> {
    let mut verts = Vec::new();
    push_plate_bevel_vertices(x, y, ww, h, r, t, sw, sh, base_color, clip_circle, &mut verts);
    verts
}

pub fn push_plate_bevel_vertices(
    x: f32, y: f32, ww: f32, h: f32,
    r: f32,
    t: f32,
    sw: f32, sh: f32,
    base_color: [f32; 4],
    clip_circle: [f32; 3],
    out: &mut Vec<Vertex>,
) {
    push_bevel_edge_vertices(x, y, ww, h, r, t, sw, sh, base_color, clip_circle, 1.0, out);
}

/// The bevel edge shading, with the light direction selectable: `light_sign` is `1.0`
/// for a raised plate (edges facing `light_source_position` are lit) and `-1.0` for a
/// recess (those same edges fall into shadow instead, and the far edges catch the
/// light). Negating the whole light vector flips every edge and every corner segment
/// consistently, because both the flat-edge factors and the arc-normal dot product
/// below are linear in it.
pub fn push_bevel_edge_vertices(
    x: f32, y: f32, ww: f32, h: f32,
    r: f32,
    t: f32,
    sw: f32, sh: f32,
    base_color: [f32; 4],
    clip_circle: [f32; 3],
    light_sign: f32,
    out: &mut Vec<Vertex>,
) {
    push_bevel_edge_vertices_radii(
        x, y, ww, h, (r, r, r, r), t, sw, sh, base_color, clip_circle, light_sign, out,
    );
}

/// As [`push_bevel_edge_vertices`], but with a per-corner radius (TL, TR, BR, BL) so the
/// lip can follow a shape whose corners differ — a recess carved along the top of a
/// rounded plate needs the plate's radius on its top corners and square ones where it
/// meets the content below. A uniform radius there would either square off the plate's
/// arc (painting a notch outside it) or wrongly round the inner corners.
pub fn push_bevel_edge_vertices_radii(
    x: f32, y: f32, ww: f32, h: f32,
    radii: (f32, f32, f32, f32),
    t: f32,
    sw: f32, sh: f32,
    base_color: [f32; 4],
    clip_circle: [f32; 3],
    light_sign: f32,
    out: &mut Vec<Vertex>,
) {
    push_bevel_edge_vertices_banded(
        x, y, ww, h, radii, t, sw, sh, base_color, clip_circle, light_sign,
        default_bevel_bands(t), (true, true, true, true), EdgeKind::Rim, out,
    );
}

/// What kind of height change an edge represents. The two shade differently because they
/// are different shapes, and using one where the other belongs is what makes a bevel read
/// as a drawn line instead of a surface.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EdgeKind {
    /// The surface *ends* here: a quarter-round rolling from face-on at the inner edge of
    /// the lip to fully in-plane at the outer boundary, where it drops away. The shading
    /// therefore peaks exactly at the boundary and dies inward. This is a plate's outer
    /// perimeter.
    Rim,
    /// The surface *continues* at a different height: one plateau steps down to another.
    /// A height field that falls monotonically across the transition has its normal tilted
    /// toward the low side the whole way, steepest in the middle and flat at both ends —
    /// so the shading is a bump straddling the boundary, not a band butted against it.
    /// Hanging the band on one side instead leaves the seam the eye reads as a drawn line.
    Step,
}

/// Shading across an edge at signed distance `d` from the boundary (positive = toward the
/// shape's interior), for a transition of width `t`. Returns the light term as a fraction
/// of full tilt.
#[inline]
fn bevel_profile(kind: EdgeKind, d: f32, t: f32) -> f32 {
    if t <= 0.0 {
        return 0.0;
    }
    match kind {
        // Normal rotates from in-plane (d = 0) to face-on (d = t): sine of what tilt is
        // left. A linear ramp here reads as a flat 45° chamfer instead of a roll.
        EdgeKind::Rim => ((1.0 - (d / t).clamp(0.0, 1.0)) * std::f32::consts::FRAC_PI_2).sin(),
        // Symmetric bump over [-t/2, +t/2], zero at both ends so the transition blends into
        // both plateaus with no seam.
        EdgeKind::Step => {
            let s = (d / t + 0.5).clamp(0.0, 1.0);
            (s * std::f32::consts::PI).sin()
        }
    }
}

/// The light-independent curvature term at signed distance `d` — the second depth cue,
/// on top of the directional one. Curvature shading is what ambient light does: convex
/// surface catches it from everywhere (bright), concave is self-occluded (dark). Because
/// it does not rotate with the light, it survives exactly where the directional term
/// dies — walls parallel to the light vector — so no edge ever vanishes entirely.
///
/// `high_sign` is +1 when the rect interior is the HIGH side of the transition and -1
/// when it is the low side (a recess). Geometry, not lighting: it does not flip with
/// `light_sign`... except that for these 2.5D shapes the two are the same number, since
/// a raised shape is lit like a plateau and shaded like one.
#[inline]
fn bevel_curvature(kind: EdgeKind, d: f32, t: f32, high_sign: f32) -> f32 {
    if t <= 0.0 {
        return 0.0;
    }
    match kind {
        // A rim is convex everywhere, tightest right at the silhouette: a bright crest
        // line hugging the boundary and dying fast inward. This is the line that makes
        // glass read as glass — the edge catches ambient light all the way around, even
        // (dimmer, via the gain asymmetry below) on the side facing away from the light.
        EdgeKind::Rim => {
            let u = (d / t).clamp(0.0, 1.0);
            let f = 1.0 - u;
            CREST_RATIO * f * f * f
        }
        // An S-curve step is convex on its high half (the shoulder) and concave on its
        // low half (the fillet, where the wall meets the floor): antisymmetric, zero at
        // the ends (no seam against either plateau) and at the midpoint.
        EdgeKind::Step => {
            let s = (d / t + 0.5).clamp(0.0, 1.0);
            let outer_is_high = -high_sign; // d < 0 is outside the rect
            // sin(2πs) is positive on the outer half — the shoulder when the outside is
            // the high side — and negative on the inner (fillet) half.
            AO_RATIO * outer_is_high * (s * std::f32::consts::TAU).sin()
        }
    }
}

/// Crest amplitude as a fraction of `bevel_depth` — how much brighter a rim's silhouette
/// line is than flat surface under even light. Must stay clearly below ~0.7 (the
/// projection of a 135° light onto an axis edge), or it cancels the directional shadow
/// on the dark side and the rim goes flat there instead of showing a faint bright line
/// over a shadowed roll.
const CREST_RATIO: f32 = 0.4;
/// Shoulder/fillet amplitude as a fraction of `bevel_depth`.
const AO_RATIO: f32 = 0.6;
/// Per-sign overlay gains. These are asymmetric the opposite way from intuition: on the
/// dark bases this DE runs, white-over blending (`b + a(1-b)`) moves the pixel far more
/// per unit alpha than black-over (`b(1-a)`) — a dark surface has little brightness for
/// black to take away. The old subtractive shading effectively crushed shadow sides to
/// black in linear space; the black overlay needs a high gain to keep shadows reading
/// at all, while white needs damping to keep highlights from blowing out.
const LIGHT_GAIN: f32 = 0.7;
const DARK_GAIN: f32 = 3.0;

/// A shading value (already scaled by `bevel_depth`) as the two overlay passes: the lit
/// pass is translucent white, the shadow pass translucent black. Painting the
/// *modulation* instead of a resolved surface color is what lets relief primitives compose — a step
/// crossing a rim shades the rim's gradient instead of stamping a flat band over it, a
/// lip on a translucent plate no longer doubles its opacity, and a recess needs no
/// knowledge of the surface color it carves.
///
/// Why two passes with fixed RGB rather than one signed color: a primitive whose value
/// crosses zero inside a band would interpolate white→black through mid-gray at
/// non-negligible alpha — on a dark base a *brightening* artifact right where the
/// shading should vanish. With per-pass alphas clamped at the crossing, each pass fades
/// to zero there and the hue can never be wrong. Alphas also stay non-negative on every
/// vertex, which the renderer requires (negative alpha is the blur sentinel).
#[inline]
fn overlay_light(v: f32) -> [f32; 4] {
    [1.0, 1.0, 1.0, (v.max(0.0) * LIGHT_GAIN).min(1.0)]
}
#[inline]
fn overlay_dark(v: f32) -> [f32; 4] {
    [0.0, 0.0, 0.0, ((-v).max(0.0) * DARK_GAIN).min(1.0)]
}

/// The signed distance range an edge's shading occupies, relative to the boundary.
#[inline]
fn bevel_span(kind: EdgeKind, t: f32) -> (f32, f32) {
    match kind {
        EdgeKind::Rim => (0.0, t),
        EdgeKind::Step => (-0.5 * t, 0.5 * t),
    }
}

/// How many gradient bands to slice a lip of thickness `t` into. Vertex colors interpolate
/// linearly, so each band is a chord of the shading curve; one band per ~1.25px keeps the
/// error under a shade step without emitting geometry finer than the display resolves.
/// The cap rose with the curvature term: a step now has two features across its width
/// (shoulder and fillet), so it needs double the samples a single bump did.
fn default_bevel_bands(t: f32) -> usize {
    ((t / 1.25).ceil() as usize).clamp(1, 12)
}

/// As [`push_bevel_edge_vertices_radii`], with the band count forced and the walls
/// selectable — for callers that want a coarser or finer roll-off than thickness alone
/// implies, or that are shading a step rather than a closed shape.
///
/// `edges` is (top, right, bottom, left). Suppressing a wall matters for a region that
/// runs flush to the surface's own edge: a full-width menubar sunk into the top of a plate
/// is a *plateau one step down*, not a trough, so its only real wall is the one facing the
/// content. Drawing the other three would carve a lip along the plate's outer edge, where
/// the plate's own roll already lives, and the two would fight.
pub fn push_bevel_edge_vertices_banded(
    x: f32, y: f32, ww: f32, h: f32,
    radii: (f32, f32, f32, f32),
    t: f32,
    sw: f32, sh: f32,
    base_color: [f32; 4],
    clip_circle: [f32; 3],
    light_sign: f32,
    bands: usize,
    edges: (bool, bool, bool, bool),
    kind: EdgeKind,
    out: &mut Vec<Vertex>,
) {
    // Floored for the same reason as `plate_push_raised`'s cap: a negative
    // extent must degrade to no ring, not panic in `clamp`.
    let cap = (ww.min(h) * 0.5).max(0.0);
    let (tl, tr, br, bl) = (
        radii.0.clamp(0.0, cap),
        radii.1.clamp(0.0, cap),
        radii.2.clamp(0.0, cap),
        radii.3.clamp(0.0, cap),
    );
    let t = t.clamp(0.0, cap);
    if t <= 0.0 {
        return;
    }
    let bands = bands.max(1);

    let rad = crate::layout::light_source_position();
    let lx = rad.cos() * light_sign;
    let ly = -rad.sin() * light_sign;
    let depth = crate::layout::bevel_depth();

    // `base_color` is no longer painted: shading is an overlay (see `overlay_color`), so
    // the surface below shows through with its own gradients and translucency intact.
    let _ = base_color;
    // Shading (directional + curvature, scaled by bevel_depth) at signed distance `d`,
    // for an edge whose outward flat normal is `dir`. A `Step` band runs negative — it
    // straddles the boundary into the plateau outside the rect, which is exactly what
    // removes the seam.
    let value = |dot: f32, d: f32| {
        depth * (bevel_profile(kind, d, t) * dot + bevel_curvature(kind, d, t, light_sign))
    };
    // The (up to two) overlay color pairs for a band running from value `v0` to `v1`:
    // one white pair and/or one black pair, each pass fading to zero alpha wherever the
    // value has the other sign. Both fire only when the band straddles the terminator.
    let passes = |v0: f32, v1: f32| -> [Option<([f32; 4], [f32; 4])>; 2] {
        [
            (v0 > 0.0 || v1 > 0.0).then(|| (overlay_light(v0), overlay_light(v1))),
            (v0 < 0.0 || v1 < 0.0).then(|| (overlay_dark(v0), overlay_dark(v1))),
        ]
    };
    let (span_lo, span_hi) = bevel_span(kind, t);

    // Each flat edge spans between its two adjoining corner radii, not a single uniform
    // inset — that is what lets the corners differ. At a square corner there is no arc to
    // cover the t×t patch where two edges meet, so the horizontal edges claim it (they run
    // the full span) and the vertical ones inset by `t`; overlapping them instead would
    // double-blend that patch, which shows as a dark notch on a translucent surface.
    let (left_top, left_bot) = (if tl > 0.0 { tl } else { t }, if bl > 0.0 { bl } else { t });
    let (right_top, right_bot) = (if tr > 0.0 { tr } else { t }, if br > 0.0 { br } else { t });
    let top_w = ww - tl - tr;
    let bottom_w = ww - bl - br;
    let left_h = h - left_top - left_bot;
    let right_h = h - right_top - right_bot;

    for k in 0..bands {
        let d0 = span_lo + (span_hi - span_lo) * (k as f32 / bands as f32);
        let d1 = span_lo + (span_hi - span_lo) * ((k + 1) as f32 / bands as f32);
        let bw = d1 - d0;

        // Top: outward normal (0,-1); the gradient runs downward, into the surface.
        if top_w > 0.0 && edges.0 {
            let (v0, v1) = (value(-ly, d0), value(-ly, d1));
            for (c0, c1) in passes(v0, v1).into_iter().flatten() {
                out.extend_from_slice(&quad_vertices_shaded(
                    x + tl, y + d0, top_w, bw, sw, sh, c0, c0, c1, c1, clip_circle,
                ));
            }
        }
        // Bottom: outward normal (0,1); gradient runs upward.
        if bottom_w > 0.0 && edges.2 {
            let (v0, v1) = (value(ly, d0), value(ly, d1));
            for (c0, c1) in passes(v0, v1).into_iter().flatten() {
                out.extend_from_slice(&quad_vertices_shaded(
                    x + bl, y + h - d1, bottom_w, bw, sw, sh, c1, c1, c0, c0, clip_circle,
                ));
            }
        }
        // Left: outward normal (-1,0); gradient runs rightward.
        if left_h > 0.0 && edges.3 {
            let (v0, v1) = (value(-lx, d0), value(-lx, d1));
            for (c0, c1) in passes(v0, v1).into_iter().flatten() {
                out.extend_from_slice(&quad_vertices_shaded(
                    x + d0, y + left_top, bw, left_h, sw, sh, c0, c1, c1, c0, clip_circle,
                ));
            }
        }
        // Right: outward normal (1,0); gradient runs leftward.
        if right_h > 0.0 && edges.1 {
            let (v0, v1) = (value(lx, d0), value(lx, d1));
            for (c0, c1) in passes(v0, v1).into_iter().flatten() {
                out.extend_from_slice(&quad_vertices_shaded(
                    x + ww - d1, y + right_top, bw, right_h, sw, sh, c1, c0, c0, c1, clip_circle,
                ));
            }
        }
    }

    // A corner arc belongs to both of its adjoining walls, so it is drawn only when both
    // are — otherwise a suppressed wall would still get a quarter of a lip.
    let corners = [
        (x + tl, y + tl, tl, std::f32::consts::PI, 1.5 * std::f32::consts::PI, edges.0 && edges.3), // Top-Left
        (x + ww - tr, y + tr, tr, 1.5 * std::f32::consts::PI, 2.0 * std::f32::consts::PI, edges.0 && edges.1), // Top-Right
        (x + ww - br, y + h - br, br, 0.0, 0.5 * std::f32::consts::PI, edges.2 && edges.1), // Bottom-Right
        (x + bl, y + h - bl, bl, 0.5 * std::f32::consts::PI, std::f32::consts::PI, edges.2 && edges.3), // Bottom-Left
    ];

    for &(cx, cy, r, start_angle, end_angle, enabled) in &corners {
        // A square corner has no arc to sweep — the flat edges already met there.
        if r <= 0.0 || !enabled {
            continue;
        }
        // The corner is a quarter of a torus: shading varies along the sweep (the normal
        // swings through 90° of the light) *and* across the lip (the roll-off). Both come
        // out of the vertex colors, so one quad per (segment × band) cell is enough — no
        // faceting, unlike the 16 flat wedges this replaced.
        let segments = ((r * 0.75) as usize).clamp(8, 48);
        let ct = t.min(r);
        for j in 0..segments {
            let theta0 = start_angle + (j as f32) * (end_angle - start_angle) / (segments as f32);
            let theta1 = start_angle + ((j + 1) as f32) * (end_angle - start_angle) / (segments as f32);
            let (cos0, sin0) = (theta0.cos(), theta0.sin());
            let (cos1, sin1) = (theta1.cos(), theta1.sin());
            for k in 0..bands {
                let d0 = span_lo + (span_hi - span_lo) * (k as f32 / bands as f32);
                let d1 = span_lo + (span_hi - span_lo) * ((k + 1) as f32 / bands as f32);
                // Inward along the corner's radius is the same signed distance as inward
                // from a flat edge, so the arc scales the span the same way.
                let (r0, r1) = (r - ct * (d0 / t), r - ct * (d1 / t));
                let p = |rho: f32, c: f32, s: f32| -> [f32; 2] {
                    [
                        ((cx + rho * c) / sw) * 2.0 - 1.0,
                        1.0 - ((cy + rho * s) / sh) * 2.0,
                    ]
                };
                // Outer/inner × the two sweep ends; each vertex gets its own value, and
                // the cell is drawn once per overlay pass that has any coverage.
                let vals = [
                    value(cos0 * lx + sin0 * ly, d0),
                    value(cos1 * lx + sin1 * ly, d0),
                    value(cos1 * lx + sin1 * ly, d1),
                    value(cos0 * lx + sin0 * ly, d1),
                ];
                let geo = [
                    p(r0, cos0, sin0),
                    p(r0, cos1, sin1),
                    p(r1, cos1, sin1),
                    p(r1, cos0, sin0),
                ];
                let mut cells: [Option<fn(f32) -> [f32; 4]>; 2] = [None, None];
                if vals.iter().any(|&v| v > 0.0) {
                    cells[0] = Some(overlay_light);
                }
                if vals.iter().any(|&v| v < 0.0) {
                    cells[1] = Some(overlay_dark);
                }
                for f in cells.into_iter().flatten() {
                    let c: Vec<Vertex> = (0..4)
                        .map(|i| Vertex { position: geo[i], color: f(vals[i]), clip_circle })
                        .collect();
                    out.extend_from_slice(&[c[0], c[1], c[2], c[0], c[2], c[3]]);
                }
            }
        }
    }
}

/// How strong the face gradient is, as a fraction of `bevel_depth` at the corner nearest
/// the light. Deliberately well below the edge amplitude: the face is a plane, not a
/// roll — it only *leans* toward the light.
const FACE_RATIO: f32 = 0.35;

/// The face lighting of a plate: a single diagonal luminance gradient across the whole
/// surface, brightest at the corner facing `light_source_position` and darkest at the
/// opposite one. This is the difference between an object and a sticker: a real surface
/// under directional light is never uniform, and a perfectly flat fill makes the eye
/// read the (much smaller) edge shading as frame decoration rather than shape.
///
/// Emitted as the same two-pass white/black overlays as the relief primitives (see
/// [`overlay_light`]/[`overlay_dark`]): fixed RGB per pass, per-corner alphas clamped at
/// the terminator, bilinear across the quad. The quad is square — its corners poke past
/// a rounded plate's arcs — but the compositor clips the window surface to the same
/// radius, so the overhang never reaches the screen.
pub fn push_plate_face_vertices(
    x: f32, y: f32, ww: f32, h: f32,
    sw: f32, sh: f32,
    clip_circle: [f32; 3],
    out: &mut Vec<Vertex>,
) {
    let rad = crate::layout::light_source_position();
    let (lx, ly) = (rad.cos(), -rad.sin());
    let amp = crate::layout::bevel_depth() * FACE_RATIO;
    // Corner value = how much its outward diagonal faces the light.
    let inv = std::f32::consts::FRAC_1_SQRT_2;
    let v_tl = amp * inv * (-lx - ly);
    let v_tr = amp * inv * (lx - ly);
    let v_br = amp * inv * (lx + ly);
    let v_bl = amp * inv * (-lx + ly);
    let vs = [v_tl, v_tr, v_br, v_bl];
    if vs.iter().any(|&v| v > 0.0) {
        out.extend_from_slice(&quad_vertices_shaded(
            x, y, ww, h, sw, sh,
            overlay_light(v_tl), overlay_light(v_tr), overlay_light(v_br), overlay_light(v_bl),
            clip_circle,
        ));
    }
    if vs.iter().any(|&v| v < 0.0) {
        out.extend_from_slice(&quad_vertices_shaded(
            x, y, ww, h, sw, sh,
            overlay_dark(v_tl), overlay_dark(v_tr), overlay_dark(v_br), overlay_dark(v_bl),
            clip_circle,
        ));
    }
}

pub fn push_plate_solid_border_vertices(
    x: f32, y: f32, ww: f32, h: f32,
    radii: crate::widget::CornerRadii,
    t: f32,
    sw: f32, sh: f32,
    color: [f32; 4],
    clip_circle: [f32; 3],
    out: &mut Vec<Vertex>,
) {
    let mut r_tl = radii.top_left.max(0.0);
    let mut r_tr = radii.top_right.max(0.0);
    let mut r_br = radii.bottom_right.max(0.0);
    let mut r_bl = radii.bottom_left.max(0.0);

    // Simple scale clamping
    let sum_top = r_tl + r_tr;
    if sum_top > ww {
        let f = ww / sum_top;
        r_tl *= f;
        r_tr *= f;
    }
    let sum_bottom = r_bl + r_br;
    if sum_bottom > ww {
        let f = ww / sum_bottom;
        r_bl *= f;
        r_br *= f;
    }
    let sum_left = r_tl + r_bl;
    if sum_left > h {
        let f = h / sum_left;
        r_tl *= f;
        r_bl *= f;
    }
    let sum_right = r_tr + r_br;
    if sum_right > h {
        let f = h / sum_right;
        r_tr *= f;
        r_br *= f;
    }

    out.extend_from_slice(&quad_vertices_with_clip(x + r_tl, y, ww - r_tl - r_tr, t, sw, sh, color, clip_circle));
    out.extend_from_slice(&quad_vertices_with_clip(x, y + r_tl, t, h - r_tl - r_bl, sw, sh, color, clip_circle));
    out.extend_from_slice(&quad_vertices_with_clip(x + r_bl, y + h - t, ww - r_bl - r_br, t, sw, sh, color, clip_circle));
    out.extend_from_slice(&quad_vertices_with_clip(x + ww - t, y + r_tr, t, h - r_tr - r_br, sw, sh, color, clip_circle));

    let segments = 16;
    let corner_e = 2.0 / crate::layout::corner_shape();

    // Corner strokes as annulus strips between the outer superellipse (radius
    // r) and its inner scaled copy (r - t): at 1px thickness the scaled inner
    // curve is indistinguishable from the true parallel curve, and at
    // corner_shape 2 this is exactly the circular arc annulus. NOT
    // push_arc_background_vertices — that stays circular for genuine arcs.
    let corner = |cx: f32, cy: f32, r: f32, start: f32, end: f32, out: &mut Vec<Vertex>| {
        let r_in = (r - t).max(0.0);
        let ndc = |px: f32, py: f32| [(px / sw) * 2.0 - 1.0, 1.0 - (py / sh) * 2.0];
        for i in 0..segments {
            let t1 = start + (i as f32) * (end - start) / segments as f32;
            let t2 = start + ((i + 1) as f32) * (end - start) / segments as f32;
            let (c1, s1) = superellipse_pt(t1, corner_e);
            let (c2, s2) = superellipse_pt(t2, corner_e);
            let o1 = ndc(cx + r * c1, cy + r * s1);
            let o2 = ndc(cx + r * c2, cy + r * s2);
            let i1 = ndc(cx + r_in * c1, cy + r_in * s1);
            let i2 = ndc(cx + r_in * c2, cy + r_in * s2);
            out.push(Vertex { position: o1, color, clip_circle });
            out.push(Vertex { position: o2, color, clip_circle });
            out.push(Vertex { position: i1, color, clip_circle });
            out.push(Vertex { position: o2, color, clip_circle });
            out.push(Vertex { position: i2, color, clip_circle });
            out.push(Vertex { position: i1, color, clip_circle });
        }
    };

    if r_tl > 0.1 {
        corner(x + r_tl, y + r_tl, r_tl, std::f32::consts::PI, 1.5 * std::f32::consts::PI, out);
    }
    if r_tr > 0.1 {
        corner(x + ww - r_tr, y + r_tr, r_tr, 1.5 * std::f32::consts::PI, 2.0 * std::f32::consts::PI, out);
    }
    if r_br > 0.1 {
        corner(x + ww - r_br, y + h - r_br, r_br, 0.0, 0.5 * std::f32::consts::PI, out);
    }
    if r_bl > 0.1 {
        corner(x + r_bl, y + h - r_bl, r_bl, 0.5 * std::f32::consts::PI, std::f32::consts::PI, out);
    }
}

pub fn push_plate_solid_border_vertices_legacy(
    x: f32, y: f32, ww: f32, h: f32,
    r: f32,
    t: f32,
    sw: f32, sh: f32,
    color: [f32; 4],
    clip_circle: [f32; 3],
    out: &mut Vec<Vertex>,
) {
    let radii = crate::widget::CornerRadii::uniform(r);
    push_plate_solid_border_vertices(x, y, ww, h, radii, t, sw, sh, color, clip_circle, out);
}

pub fn widget_vertices(w: &dyn crate::widget::WidgetHost, sw: f32, sh: f32, clip_circle: [f32; 3]) -> Vec<Vertex> {
    let mut verts = Vec::new();
    push_widget_vertices(w, sw, sh, clip_circle, &mut verts);
    verts
}

pub fn push_widget_vertices(w: &dyn crate::widget::WidgetHost, sw: f32, sh: f32, clip_circle: [f32; 3], out: &mut Vec<Vertex>) {
    let (x, y, ww, h) = w.rect();
    let radii = w.corner_radii();
    if let Some(thickness) = w.plate_bevel() {
        let t = thickness;
        // Full-size fill: the bevel lip is a shading overlay now, not a paint of the
        // outer ring, so an inset fill would leave the ring unfilled.
        push_rounded_rect_vertices_corners(x, y, ww, h, radii, sw, sh, w.color(), clip_circle, None, out);
        push_plate_bevel_vertices(x, y, ww, h, radii.top_left, t, sw, sh, w.color(), clip_circle, out);
    } else {
        push_rounded_rect_vertices_corners(x, y, ww, h, radii, sw, sh, w.color(), clip_circle, None, out);
        if let Some((color, thickness)) = w.solid_border() {
            push_plate_solid_border_vertices(x, y, ww, h, radii, thickness, sw, sh, color, clip_circle, out);
        }
    }

    for (cx, cy, r, t, start, end, qc) in w.extra_arcs() {
        push_arc_background_vertices(cx, cy, r, t, start, end, sw, sh, qc, 16, clip_circle, out);
    }
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
    /// [`crate::vk::PlatePush`]; already in physical px). Never merged.
    pub plate: Option<crate::vk::PlatePush>,
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
/// `CCE_PLATE_DEBUG=1` — trace which carves group into their host plate as exact
/// CSG features and which fall back to the standalone overlay shading.
///
/// The two paths do NOT look the same: a grouped carve is part of the plate's
/// single height field, so its wall meets the plate's rolled perimeter as a real
/// junction, while the fallback approximates that with the host-box fade. Six
/// conditions decide it, three of them dynamic (draw order, neighbouring plates,
/// whether another carve already claimed the host's feature run), so the SAME
/// widget can render either way depending on what is around it — and it does so
/// silently. That has already shipped as a bug once: a hovered button's opaque
/// fill used to sever every later button from the root plate they carve into,
/// which is why `plate_stack` is a stack (see its comment below).
///
/// Off by default and read once; the classification below runs only when set.
/// Prim discriminant name, for `CCE_PLATE_DEBUG` reporting only.
fn prim_kind(p: &crate::scene::paint::Prim) -> &'static str {
    use crate::scene::paint::Prim as P;
    match p {
        P::Quad { .. } => "Quad", P::RoundedRect { .. } => "RoundedRect",
        P::Border { .. } => "Border", P::Bevel { .. } => "Bevel",
        P::Recess { .. } => "Recess", P::Boss { .. } => "Boss",
        P::Ridge { .. } => "Ridge", P::Trough { .. } => "Trough", P::Plate { .. } => "Plate",
        P::Arc { .. } => "Arc", P::ArcShaded { .. } => "ArcShaded",
        P::Vector { .. } => "Vector", P::Circle { .. } => "Circle",
        P::Sphere { .. } => "Sphere", P::Droplet { .. } => "Droplet",
        P::DropletScrim { .. } => "DropletScrim",
        P::ConcaveFillet { .. } => "ConcaveFillet",
        P::Groove { .. } => "Groove", P::Lattice { .. } => "Lattice",
        P::CarveUnion { .. } => "CarveUnion", P::Glow { .. } => "Glow",
        P::Text { .. } => "Text", P::Image { .. } => "Image",
    }
}

fn plate_debug() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var("CCE_PLATE_DEBUG").is_ok_and(|v| v != "0"))
}

/// Debug builds make one kind of fallback LOUD without `CCE_PLATE_DEBUG`: a
/// carve that could group (full ring, untinted) failing to while a still-open
/// plate encloses it and the carve's shaded region reaches that plate's
/// perimeter roll. There the grouped and overlay paths shade the junction
/// differently, and the rejection is one of the dynamic rules — so the SAME
/// widget can flip looks frame to frame with nothing on stderr. Not an
/// assert/panic: every rejection is conservative-CORRECT (the audit that
/// shipped CCE_PLATE_DEBUG found no misgrouping; a later plate overlapping the
/// carve genuinely must be shaded over, not under) — it is the frame-to-frame
/// LOOK that flips, so the right loudness is an unmissable warning, not a
/// crash. The ubiquitous quiet case stays quiet by construction: ordinary
/// geometry closing every grouping window empties `plate_stack`, so no
/// enclosing OPEN plate exists and this never runs — that is draw-order
/// design, not a flip.
///
/// Returns the dynamic rule to report, or `None` when the fallback is not the
/// loud case. Pure so the classification is unit-testable; `later_plates` are
/// the open plates emitted after the enclosing host.
#[cfg(debug_assertions)]
fn near_roll_fallback_reason(
    carve: &crate::scene::layout::Rect,
    depth: f32,
    host: &crate::scene::layout::Rect,
    roll: f32,
    later_plates: &[crate::scene::layout::Rect],
    budget_full: bool,
) -> Option<&'static str> {
    // The carve's shaded region — the overlay path's cover-quad inflation.
    let infl = depth * 0.5 + 2.0;
    let (sx0, sy0) = (carve.x - infl, carve.y - infl);
    let (sx1, sy1) = (carve.x + carve.width + infl, carve.y + carve.height + infl);
    // "Near the roll" = the shaded region leaves the host rect deflated by the
    // host's own roll width on any side.
    let near = sx0 < host.x + roll
        || sy0 < host.y + roll
        || sx1 > host.x + host.width - roll
        || sy1 > host.y + host.height - roll;
    if !near {
        return None;
    }
    // The dynamic rules, in the order the grouping guard tests them.
    if budget_full {
        return Some("the feature budget is full");
    }
    if later_plates
        .iter()
        .any(|o| sx0 < o.x + o.width && sx1 > o.x && sy0 < o.y + o.height && sy1 > o.y)
    {
        return Some("a later plate overlaps the carve's shaded region");
    }
    Some("the host's feature run is closed (another plate appended features since)")
}

/// Print a near-roll fallback warning once per distinct message — a carve in a
/// steady layout would otherwise repeat it every frame.
#[cfg(debug_assertions)]
fn plate_carve_warn_once(msg: String) {
    use std::sync::{Mutex, OnceLock};
    static SEEN: OnceLock<Mutex<std::collections::HashSet<String>>> = OnceLock::new();
    let seen = SEEN.get_or_init(|| Mutex::new(std::collections::HashSet::new()));
    if seen.lock().unwrap().insert(msg.clone()) {
        eprintln!("{msg}");
    }
}

pub fn tessellate_display_list(
    dl: &crate::scene::paint::DisplayList,
    sw: f32,
    sh: f32,
    scale: f32,
) -> (Vec<Vertex>, Vec<DlBatch>, Vec<DlImage>, Vec<[f32; 12]>) {
    use crate::scene::material::PlateRole;
    use crate::scene::paint::{Cap, Prim};
    let mut verts: Vec<Vertex> = Vec::new();
    let mut batches: Vec<DlBatch> = Vec::new();
    let mut images: Vec<DlImage> = Vec::new();
    // Carves CSG'd into plates (see Frame2D::plate_features), plus the plate
    // they group into: the most recent Plate/Bevel batch, provided only Text
    // and Image prims (which draw through separate paths anyway) intervene.
    let mut features: Vec<[f32; 12]> = Vec::new();
    // Open carve-host plates, in emission order (innermost candidates last).
    // A STACK, not a single slot: a sibling plate emitted between a root plate
    // and its later carves (a hovered button's opaque fill among transparent
    // ones) must not sever those carves from the root plate they are carved
    // into — that severing rendered every button after the hovered one
    // through the visually-different overlay fallback. Ordinary geometry
    // still closes every open plate (the draw-order rule below).
    let mut plate_stack: Vec<(usize, crate::scene::layout::Rect)> = Vec::new();
    // Which plate last appended a carve feature: a plate's features are
    // addressed as one contiguous [offset, count] run (PlatePush::host), so a
    // plate may only receive MORE features while no other plate has appended
    // any since.
    let mut last_feature_plate: Option<usize> = None;
    // `CCE_PLATE_DEBUG` bookkeeping — see `plate_debug`.
    let dbg_plates = plate_debug();
    let mut dbg_grouped = 0usize;
    let mut dbg_fell_back: Vec<String> = Vec::new();
    let mut dbg_opened = 0usize;
    // Which prim kind closed a still-open grouping window, and how many plates
    // it closed — the answer to "why was there no enclosing plate?".
    let mut dbg_closed_by: std::collections::BTreeMap<&'static str, usize> =
        std::collections::BTreeMap::new();

    // SDF-lit plate path (shader2d's plate branch) vs the legacy banded vertex
    // shading, plus the frame-constant lighting inputs it pushes per plate.
    let shader_plates = crate::layout::bevel_shader();
    // Light and material come from `scene::relief_shade`, which is also what
    // cce-relief predicts pixels with — one definition, so the editor cannot
    // draw a different material than the renderer applies.
    let plate_light = crate::scene::relief_shade::light_vector();
    // [shading strength (1.0 at the default bevel_depth), specular strength,
    // shininess, curvature/AO strength] — the DE's finish, for the CARVES,
    // which shade whatever is beneath them and so take the host's. A prim
    // that carries a Material (Plate, Bevel, Sphere, Droplet) pushes its own
    // `material.finish` instead. Curvature is kept near the raised path's
    // crest amplitude: the recess shoulder's brightening lands on the same
    // pixels as its specular line, and the two stack — at 0.5 the step read
    // several times hotter than a plate roll.
    let plate_mat = crate::scene::material::Finish::from_style().to_array();

    for item in &dl.items {
        let mut start = verts.len() as u32;
        let mut plate: Option<crate::vk::PlatePush> = None;
        // A frosted flat fill promoted to a zero-depth plate batch (below):
        // it carries a recipe like any plate, but it is ordinary geometry to
        // the carve grouping — it opens no host and closes the open ones.
        let mut promoted = false;
        let mut made_plate: Option<crate::scene::layout::Rect> = None;
        // Blur-behind marker: a prim whose FILL alpha is negative asks the
        // renderer to snapshot the frame-so-far before it draws. Every
        // fill-bearing prim counts — the shader's a<0 branch runs for all of
        // them, and a variant missing here still frosts, but against the
        // stale scene backdrop instead of the frame: a flat tint with no
        // content and no blur, which is how the Dropdown popover (Border)
        // and the menubar panels (Quad) shipped visibly unfrosted while the
        // context menu (Plate) worked.
        let mut blur_behind = matches!(
            &item.prim,
            crate::scene::paint::Prim::Quad { color, .. }
            | crate::scene::paint::Prim::RoundedRect { color, .. } if color[3] < 0.0
        ) || matches!(
            &item.prim,
            crate::scene::paint::Prim::Bevel { material, .. }
            | crate::scene::paint::Prim::Plate { material, .. }
            | crate::scene::paint::Prim::Droplet { material, .. }
                if material.fill(PlateRole::Nested)[3] < 0.0
        ) || matches!(
            &item.prim,
            crate::scene::paint::Prim::Border { fill, .. } if fill[3] < 0.0
        );
        // Logical [cx, cy, r] → the physical-pixel triple the vertex attribute carries.
        let no = item
            .clip_circle
            .map(|c| [c[0] * scale, c[1] * scale, c[2] * scale])
            .unwrap_or([0.0f32, 0.0, 0.0]);
        // Fixed 16-segment fans read as polygons once a circle/arc is pane-sized; scale
        // the fan with the PHYSICAL radius (capped — beyond 128 the chord error is
        // subpixel even on HiDPI).
        let segs = |radius: f32| -> usize { ((radius * scale) as usize).clamp(16, 128) };
        match &item.prim {
            Prim::Text { .. } => continue, // text goes through the glyph/text-span path
            Prim::Image { image, rect, alpha } => {
                images.push(DlImage {
                    image: *image,
                    rect: *rect,
                    alpha: *alpha,
                    at: verts.len() as u32,
                    clip: item.clip,
                });
                continue;
            }
            // A frosted FLAT fill — a `Flat` control face, a menu panel, a
            // popover, an inset plate's face — is a zero-depth plate batch
            // (RFC material § 6.2): the same shader path as every plate, so
            // it carries its own frost recipe instead of a window-wide one,
            // with circular corners (shape 2) and no roll, which is what the
            // tessellated fill drew. The display list is untouched, so the
            // legacy bridges that extract RoundedRects still see one.
            Prim::Quad { rect, color } if shader_plates && color[3] < 0.0 => {
                verts.extend(quad_vertices(rect.x, rect.y, rect.width, rect.height, sw, sh, *color));
                plate = Some(flat_frost_push(rect, (0.0, 0.0, 0.0, 0.0), *color, scale, plate_light, plate_mat));
                promoted = true;
            }
            Prim::RoundedRect { rect, radius, corners, color } if shader_plates && color[3] < 0.0 => {
                let radii = (
                    if corners.0 { *radius } else { 0.0 },
                    if corners.1 { *radius } else { 0.0 },
                    if corners.2 { *radius } else { 0.0 },
                    if corners.3 { *radius } else { 0.0 },
                );
                verts.extend(quad_vertices(rect.x, rect.y, rect.width, rect.height, sw, sh, *color));
                plate = Some(flat_frost_push(rect, radii, *color, scale, plate_light, plate_mat));
                promoted = true;
            }
            Prim::Border { rect, radii, fill, border, thickness } if shader_plates && fill[3] < 0.0 => {
                // The fill as its own plate batch, closed here; the stroke
                // follows as ordinary geometry in the batch the tail makes.
                verts.extend(quad_vertices(rect.x, rect.y, rect.width, rect.height, sw, sh, *fill));
                let p = flat_frost_push(rect, *radii, *fill, scale, plate_light, plate_mat);
                let end = verts.len() as u32;
                plate_stack.clear();
                batches.push(DlBatch { scissor: item.clip, clip_rrect: item.clip_rrect, start, end, plate: Some(p), blur_behind: true });
                start = end;
                blur_behind = false;
                let cr = crate::widget::CornerRadii::new(radii.0, radii.1, radii.2, radii.3);
                push_plate_solid_border_vertices(rect.x, rect.y, rect.width, rect.height, cr, *thickness, sw, sh, *border, no, &mut verts);
            }
            Prim::Quad { rect, color } => {
                // Quads honor an active circle clip like circles/arcs do (the
                // Ramp's foam-cell fills draw as clipped strips).
                verts.extend(quad_vertices_with_clip(rect.x, rect.y, rect.width, rect.height, sw, sh, *color, no));
            }
            Prim::RoundedRect { rect, radius, corners, color } => {
                let radii = crate::widget::CornerRadii::new(
                    if corners.0 { *radius } else { 0.0 },
                    if corners.1 { *radius } else { 0.0 },
                    if corners.2 { *radius } else { 0.0 },
                    if corners.3 { *radius } else { 0.0 },
                );
                push_rounded_rect_vertices_corners(rect.x, rect.y, rect.width, rect.height, radii, sw, sh, *color, no, None, &mut verts);
            }
            Prim::Border { rect, radii, fill, border, thickness } => {
                let cr = crate::widget::CornerRadii::new(radii.0, radii.1, radii.2, radii.3);
                push_rounded_rect_vertices_corners(rect.x, rect.y, rect.width, rect.height, cr, sw, sh, *fill, no, None, &mut verts);
                push_plate_solid_border_vertices(rect.x, rect.y, rect.width, rect.height, cr, *thickness, sw, sh, *border, no, &mut verts);
            }
            Prim::Glow { rect, radius, reach, color } => {
                push_glow_vertices(rect.x, rect.y, rect.width, rect.height, *radius, *reach, sw, sh, *color, no, &mut verts);
            }
            Prim::Bevel { rect, radii, material, depth, tint } if shader_plates => {
                let color = material.fill(PlateRole::Nested);
                let mat = material.finish.to_array();
                // SDF-lit raised plate: one cover quad; the shader owns fill,
                // roll shading, corners, and silhouette AA. Nominal corner
                // radii (scale_corners false): a Bevel is a WIDGET-scale plate
                // whose silhouette must match the nominal-radius squircles of
                // the controls around it — only window-scale `Plate`s get the
                // curvature-matched span.
                verts.extend(quad_vertices(rect.x, rect.y, rect.width, rect.height, sw, sh, color));
                let mut p = plate_push_raised(rect, *radii, *depth, scale, plate_light, mat, false, None);
                // The plate's own frost recipe rides host.zw (see PlatePush).
                let [fz, fw] = material.frost.pack(scale);
                p.host[2] = fz;
                p.host[3] = fw;
                // w = 1 marks an accent-tinted plate (the focused-pane
                // treatment): the shader then colors the WHOLE rolled edge
                // with the tint, not just the specular glint — matching the
                // free-carve path's tinted-well convention. Neutral white
                // keeps w = 0 (spec-only, a no-op multiply).
                let full = if *tint == [1.0, 1.0, 1.0] { 0.0 } else { 1.0 };
                p.specular_tint = [tint[0], tint[1], tint[2], full];
                plate = Some(p);
                made_plate = Some(*rect);
            }
            Prim::Plate { rect, radii, material, depth, shape } if shader_plates => {
                let color = material.fill(PlateRole::Nested);
                let mat = material.finish.to_array();
                if *depth < 0.0 {
                    // Negative depth = fill-less roll overlay (MODE_ROLL): the
                    // window-edge roll shading alone, screened over whatever is
                    // beneath — for a root plate whose face is not a fill (the
                    // designer's 3D canvas). The cover quad carries no color,
                    // and the batch is NOT opened as a carve host: an overlay
                    // owns no surface for a CSG feature to cut into.
                    verts.extend(quad_vertices(rect.x, rect.y, rect.width, rect.height, sw, sh, [0.0; 4]));
                    let mut p = plate_push_raised(rect, *radii, -*depth, scale, plate_light, mat, true, *shape);
                    p.mode = 11.0; // MODE_ROLL
                    plate = Some(p);
                } else {
                    // Same lit-plate branch; the cover quad is the exact rect so the
                    // silhouette and the compositor's rounded window corners agree.
                    verts.extend(quad_vertices(rect.x, rect.y, rect.width, rect.height, sw, sh, color));
                    let mut p = plate_push_raised(rect, *radii, *depth, scale, plate_light, mat, true, *shape);
                    let [fz, fw] = material.frost.pack(scale);
                    p.host[2] = fz;
                    p.host[3] = fw;
                    plate = Some(p);
                    made_plate = Some(*rect);
                }
            }
            Prim::Recess { rect, radii, depth, edges, .. }
            | Prim::Boss { rect, radii, depth, edges, .. }
            | Prim::Ridge { rect, radii, depth, edges }
            | Prim::Trough { rect, radii, depth, edges, .. }
                if shader_plates =>
            {
                let tint = match &item.prim {
                    Prim::Recess { tint, .. } => *tint,
                    Prim::Boss { tint, .. } => *tint,
                    Prim::Trough { tint, .. } => *tint,
                    _ => None,
                };
                // Recess carves down into the surface; Boss raises a plateau out
                // of it (same machinery, depth sign flipped); Ridge is a raised
                // rim straddling the boundary and Trough the sunken valley twin
                // (their own overlay profiles — never grouped, the CSG features
                // only model monotonic steps).
                let mode = match &item.prim {
                    Prim::Boss { .. } => 3.0f32,
                    Prim::Ridge { .. } => 4.0,
                    Prim::Trough { .. } => 9.0,
                    _ => 2.0,
                };
                let raised = mode > 2.5;
                // Grouped into the enclosing plate whenever one is live: the
                // carve becomes a CSG feature of that plate's single draw —
                // exact composite shading, real junctions at the plate's rolled
                // perimeter — instead of a shading overlay (the fallback below).
                //
                // Edge-suppressed carves NEVER group: a suppressed wall's rect
                // extends past the carve (below), relying on the overlay cover
                // quad to keep that shading out of the drawn pixels — a clip
                // the plate's whole-surface draw does not have, so grouped it
                // smears the extended walls across the plate. Union pieces
                // (section wells, a spinbox's field and button run) are
                // exactly these.
                // A tinted carve also never groups: a CSG feature is geometry only,
                // so the tint could only land on the whole plate's specular.
                let full_ring = *edges == (true, true, true, true);
                let host_plate = if mode < 3.5 && full_ring && tint.is_none() && features.len() < crate::vk::MAX_PLATE_FEATURES {
                    // The carve's shaded region, for the occlusion test below
                    // (the overlay path's cover-quad inflation).
                    let infl = *depth * 0.5 + 2.0;
                    let (sx0, sy0) = (rect.x - infl, rect.y - infl);
                    let (sx1, sy1) = (rect.x + rect.width + infl, rect.y + rect.height + infl);
                    plate_stack
                        .iter()
                        .enumerate()
                        .rev()
                        .find(|(si, (bi, prect))| {
                            let inside = rect.x >= prect.x - 0.5
                                && rect.y >= prect.y - 0.5
                                && rect.x + rect.width <= prect.x + prect.width + 0.5
                                && rect.y + rect.height <= prect.y + prect.height + 0.5;
                            if !inside {
                                return false;
                            }
                            // Pixels drawn since this plate (a LATER plate in the
                            // stack) must not overlap the carve — its shading would
                            // land beneath them in this plate's earlier draw.
                            if plate_stack[si + 1..].iter().any(|(_, orect)| {
                                sx0 < orect.x + orect.width
                                    && sx1 > orect.x
                                    && sy0 < orect.y + orect.height
                                    && sy1 > orect.y
                            }) {
                                return false;
                            }
                            // Contiguity: only the last feature-receiving plate (or
                            // one with no features yet) may take another.
                            batches[*bi].plate.as_ref().map_or(false, |p| p.host[1] == 0.0)
                                || last_feature_plate == Some(*bi)
                        })
                        .map(|(_, &(bi, _))| bi)
                } else {
                    None
                };
                // Debug-build loudness for the silent grouped→overlay flip —
                // see `near_roll_fallback_reason` on what qualifies and why
                // this warns instead of panicking.
                #[cfg(debug_assertions)]
                if host_plate.is_none() && mode < 3.5 && full_ring && tint.is_none() {
                    let enclosing = plate_stack.iter().enumerate().rev().find(|(_, (_, p))| {
                        rect.x >= p.x - 0.5
                            && rect.y >= p.y - 0.5
                            && rect.x + rect.width <= p.x + p.width + 0.5
                            && rect.y + rect.height <= p.y + p.height + 0.5
                    });
                    if let Some((si, &(bi, prect))) = enclosing {
                        // Host roll width rides the push's light.w (physical px).
                        let roll = batches[bi].plate.as_ref().map_or(0.0, |p| p.light[3]) / scale;
                        let later: Vec<crate::scene::layout::Rect> =
                            plate_stack[si + 1..].iter().map(|&(_, r)| r).collect();
                        let budget_full = features.len() >= crate::vk::MAX_PLATE_FEATURES;
                        if let Some(why) =
                            near_roll_fallback_reason(rect, *depth, &prect, roll, &later, budget_full)
                        {
                            let kind = if mode > 2.5 { "boss" } else { "recess" };
                            plate_carve_warn_once(format!(
                                "plate-carve: near-roll {kind} ({:.0},{:.0} {:.0}x{:.0}) lost grouping — {why}; \
                                 its junction with the host plate's roll shades through the overlay fallback, \
                                 visually different from grouped frames (CCE_PLATE_DEBUG=1 traces verdicts) \
                                 [debug-build warning, printed once]",
                                rect.x, rect.y, rect.width, rect.height
                            ));
                        }
                    }
                }
                if dbg_plates {
                    match host_plate {
                        Some(_) => dbg_grouped += 1,
                        None => {
                            // Re-derive WHY, in the same order the guard tests
                            // them. Debug-only: the hot path above is untouched.
                            let kind = match &item.prim {
                                Prim::Boss { .. } => "boss",
                                Prim::Ridge { .. } => "ridge",
                                Prim::Trough { .. } => "trough",
                                _ => "recess",
                            };
                            let infl = *depth * 0.5 + 2.0;
                            let (sx0, sy0) = (rect.x - infl, rect.y - infl);
                            let (sx1, sy1) = (rect.x + rect.width + infl, rect.y + rect.height + infl);
                            let enclosing: Vec<usize> = plate_stack
                                .iter()
                                .enumerate()
                                .filter(|(_, (_, p))| {
                                    rect.x >= p.x - 0.5
                                        && rect.y >= p.y - 0.5
                                        && rect.x + rect.width <= p.x + p.width + 0.5
                                        && rect.y + rect.height <= p.y + p.height + 0.5
                                })
                                .map(|(si, _)| si)
                                .collect();
                            let occluded = |si: usize| {
                                plate_stack[si + 1..].iter().any(|(_, o)| {
                                    sx0 < o.x + o.width && sx1 > o.x && sy0 < o.y + o.height && sy1 > o.y
                                })
                            };
                            let why = if mode >= 3.5 {
                                "ridge — never groups (its bump profile is not a monotonic step)".into()
                            } else if !full_ring {
                                format!("edge-suppressed {edges:?} — the extended wall would smear across the host")
                            } else if tint.is_some() {
                                "tinted — a CSG feature is geometry only, it carries no color".into()
                            } else if features.len() >= crate::vk::MAX_PLATE_FEATURES {
                                format!("feature budget full ({} used)", features.len())
                            } else if enclosing.is_empty() {
                                format!("no enclosing plate ({} open)", plate_stack.len())
                            } else if enclosing.iter().all(|&si| occluded(si)) {
                                "a later plate overlaps this carve's shaded region".into()
                            } else {
                                "host plate's feature run is closed (another carve appended since)".into()
                            };
                            dbg_fell_back.push(format!(
                                "  overlay: {kind} ({:.0},{:.0} {:.0}x{:.0}) — {why}",
                                rect.x, rect.y, rect.width, rect.height
                            ));
                        }
                    }
                }
                if let Some(bi) = host_plate {
                    {
                        // A wall the carve shares with the plate's edge extends
                        // past the plate, so the carve has no wall there.
                        let ext = *depth + 4.0;
                        let (mut x0, mut y0) = (rect.x, rect.y);
                        let (mut x1, mut y1) = (rect.x + rect.width, rect.y + rect.height);
                        if !edges.0 { y0 -= ext; }
                        if !edges.1 { x1 += ext; }
                        if !edges.2 { y1 += ext; }
                        if !edges.3 { x0 -= ext; }
                        let t_px = *depth * scale;
                        // The carve's drop: the material's pinned height, else
                        // the analytic ratio of the wall saturating at the DE's
                        // roll width (`layout::carve_depth_px` states the rule
                        // once for this path and the shader's free carves).
                        let k_mag = crate::layout::carve_depth_px(*depth) * scale;
                        // Negative depth = raised (Boss); the shader's summed
                        // slope vectors and curvature sign follow it.
                        let k_px = if raised { -k_mag } else { k_mag };
                        if let Some(p) = batches[bi].plate.as_mut() {
                            if p.host[1] == 0.0 {
                                p.host[0] = features.len() as f32;
                            }
                            p.host[1] += 1.0;
                        }
                        last_feature_plate = Some(bi);
                        features.push([
                            (x0 + x1) * 0.5 * scale,
                            (y0 + y1) * 0.5 * scale,
                            (x1 - x0) * 0.5 * scale,
                            (y1 - y0) * 0.5 * scale,
                            radii.0 * scale,
                            radii.1 * scale,
                            radii.2 * scale,
                            radii.3 * scale,
                            t_px,
                            k_px,
                            0.0,
                            0.0,
                        ]);
                        continue;
                    }
                }
                // Overlay-only carve: the cover quad inflates by half the roll
                // width (the step straddles the boundary) and carries no color —
                // the shader emits translucent white/black over what's beneath.
                let infl = *depth * 0.5 + 2.0;
                verts.extend(quad_vertices(
                    rect.x - infl, rect.y - infl,
                    rect.width + 2.0 * infl, rect.height + 2.0 * infl,
                    sw, sh, [0.0; 4],
                ));
                // A suppressed wall is pushed past the cover quad, so its
                // shading falls outside the drawn pixels (see Prim::Recess on
                // why a flush region is a step, not a trough).
                let ext = *depth + 4.0;
                let (mut x0, mut y0) = (rect.x, rect.y);
                let (mut x1, mut y1) = (rect.x + rect.width, rect.y + rect.height);
                if !edges.0 { y0 -= ext; }
                if !edges.1 { x1 += ext; }
                if !edges.2 { y1 += ext; }
                if !edges.3 { x0 -= ext; }
                let sdf_rect = crate::scene::layout::Rect { x: x0, y: y0, width: x1 - x0, height: y1 - y0 };
                let mut p = plate_push_raised(&sdf_rect, *radii, *depth, scale, plate_light, plate_mat, false, None);
                p.mode = mode;
                // w = 1.0 flags the free-carve shader path to mix its white
                // highlight screen toward the tint (plates leave w at 0.0).
                if let Some(t) = tint {
                    p.specular_tint = [t[0], t[1], t[2], 1.0];
                }
                // Host-plate box for the roll fade: a suppressed wall means the
                // recess runs flush to the host's edge there, so that side of
                // the box sits at the original rect edge; enabled walls face
                // host interior, pushed to ±1e5 so no fade applies.
                const FAR: f32 = 1e5;
                let (hx0, hy0) = (
                    if edges.3 { rect.x - FAR } else { rect.x },
                    if edges.0 { rect.y - FAR } else { rect.y },
                );
                let (hx1, hy1) = (
                    if edges.1 { rect.x + rect.width + FAR } else { rect.x + rect.width },
                    if edges.2 { rect.y + rect.height + FAR } else { rect.y + rect.height },
                );
                p.host = [
                    (hx0 + hx1) * 0.5 * scale,
                    (hy0 + hy1) * 0.5 * scale,
                    (hx1 - hx0) * 0.5 * scale,
                    (hy1 - hy0) * 0.5 * scale,
                ];
                plate = Some(p);
            }
            Prim::Bevel { rect, radii, material, depth, tint: _ } => {
                let color = material.fill(PlateRole::Nested);
                // Full-size fill: the lip is now a shading overlay, not a paint of the
                // outer ring, so the fill must cover the whole rect (the old inset fill
                // would leave the ring showing whatever lay beneath).
                let corners = crate::widget::CornerRadii {
                    top_left: radii.0, top_right: radii.1,
                    bottom_right: radii.2, bottom_left: radii.3,
                };
                push_rounded_rect_vertices_corners(rect.x, rect.y, rect.width, rect.height, corners, sw, sh, color, no, None, &mut verts);
                push_plate_bevel_vertices(rect.x, rect.y, rect.width, rect.height, radii.0, *depth, sw, sh, color, no, &mut verts);
            }
            Prim::Plate { rect, radii, material, depth, .. } => {
                let color = material.fill(PlateRole::Nested);
                if *depth < 0.0 {
                    // Fill-less roll overlay (negative-depth sentinel): the banded
                    // legacy tessellation has no overlay compositing, so the roll
                    // is simply absent here — the A/B path draws nothing rather
                    // than a wrong fill.
                    continue;
                }
                // Fill at full size (no inset — see Prim::Plate), then light the face,
                // then roll the perimeter. The lip rides on top of the fill's outer band
                // rather than replacing it, so the plate's silhouette and the
                // compositor's rounded window corners still agree exactly.
                let corners = crate::widget::CornerRadii {
                    top_left: radii.0, top_right: radii.1,
                    bottom_right: radii.2, bottom_left: radii.3,
                };
                push_rounded_rect_vertices_corners(
                    rect.x, rect.y, rect.width, rect.height, corners, sw, sh, color, no, None, &mut verts,
                );
                push_plate_face_vertices(rect.x, rect.y, rect.width, rect.height, sw, sh, no, &mut verts);
                push_bevel_edge_vertices_radii(
                    rect.x, rect.y, rect.width, rect.height, *radii, *depth,
                    sw, sh, color, no, 1.0, &mut verts,
                );
            }
            Prim::Recess { rect, radii, depth, edges, .. } => {
                // Edges only — no fill: the shading is an overlay, so whatever is painted
                // below (fill, rim gradient, blur) shows through the carve modulated
                // rather than repainted. `light_sign = -1.0` shadows the lit-facing edges,
                // which is the raised->recessed inversion.
                push_bevel_edge_vertices_banded(
                    rect.x, rect.y, rect.width, rect.height, *radii, *depth,
                    sw, sh, [0.0; 4], no, -1.0, default_bevel_bands(*depth), *edges,
                    EdgeKind::Step, &mut verts,
                );
            }
            Prim::Boss { rect, radii, depth, edges, .. } => {
                // Legacy raised step: the recess overlay with the light sign upright.
                push_bevel_edge_vertices_banded(
                    rect.x, rect.y, rect.width, rect.height, *radii, *depth,
                    sw, sh, [0.0; 4], no, 1.0, default_bevel_bands(*depth), *edges,
                    EdgeKind::Step, &mut verts,
                );
            }
            Prim::Ridge { rect, radii, depth, edges } => {
                // Legacy approximation: a raised step up at the boundary plus a
                // recessed step down half a width in (the banded machinery has no
                // bump profile; the double-pass hot crest is accepted here — the
                // legacy path exists only for A/B comparison).
                let half = *depth * 0.5;
                push_bevel_edge_vertices_banded(
                    rect.x, rect.y, rect.width, rect.height, *radii, half,
                    sw, sh, [0.0; 4], no, 1.0, default_bevel_bands(half), *edges,
                    EdgeKind::Step, &mut verts,
                );
                let ir = (radii.0 - half).max(0.0);
                push_bevel_edge_vertices_banded(
                    rect.x + half, rect.y + half,
                    rect.width - *depth, rect.height - *depth,
                    (ir, ir, ir, ir), half,
                    sw, sh, [0.0; 4], no, -1.0, default_bevel_bands(half), *edges,
                    EdgeKind::Step, &mut verts,
                );
            }
            Prim::Trough { rect, radii, depth, edges, .. } => {
                // Legacy approximation, the Ridge arm's two steps with the light
                // signs swapped: down at the boundary, back up half a width in.
                // The banded machinery has no valley profile, so this is the old
                // stacked look — accepted here, as the legacy path exists only
                // for A/B comparison against the SDF one.
                let half = *depth * 0.5;
                push_bevel_edge_vertices_banded(
                    rect.x, rect.y, rect.width, rect.height, *radii, half,
                    sw, sh, [0.0; 4], no, -1.0, default_bevel_bands(half), *edges,
                    EdgeKind::Step, &mut verts,
                );
                let ir = (radii.0 - half).max(0.0);
                push_bevel_edge_vertices_banded(
                    rect.x + half, rect.y + half,
                    rect.width - *depth, rect.height - *depth,
                    (ir, ir, ir, ir), half,
                    sw, sh, [0.0; 4], no, 1.0, default_bevel_bands(half), *edges,
                    EdgeKind::Step, &mut verts,
                );
            }
            Prim::Arc { cx, cy, radius, thickness, start: sa, end: ea, color } => {
                push_arc_background_vertices(*cx, *cy, *radius, *thickness, *sa, *ea, sw, sh, *color, segs(*radius), no, &mut verts);
            }
            Prim::ArcShaded { cx, cy, radius, thickness, start: sa, end: ea, inner, crest, outer } => {
                push_arc_shaded_vertices(*cx, *cy, *radius, *thickness, *sa, *ea, sw, sh, *inner, *crest, *outer, segs(*radius), no, &mut verts);
            }
            Prim::Vector { x1, y1, x2, y2, thickness, color, cap } => {
                let lc = match cap {
                    Cap::Flat => LineCap::Flat,
                    Cap::Round => LineCap::Round,
                    Cap::Arrow => LineCap::Arrow,
                };
                verts.extend(vector_vertices(*x1, *y1, *x2, *y2, *thickness, sw, sh, *color, lc));
            }
            Prim::Circle { cx, cy, radius, color } => {
                if item.clip_circle.is_none() && *radius > 1.5 {
                    // Cover quad with the disc itself as the (feathered) circle
                    // clip: a per-pixel smooth silhouette instead of a hard-edged
                    // fan. The quad overhangs by 1px for the feather. Only when
                    // no ancestor clip holds the slot — then it's the fan path.
                    let own = [cx * scale, cy * scale, radius * scale];
                    let d = *radius + 1.0;
                    verts.extend(quad_vertices_with_clip(
                        cx - d, cy - d, 2.0 * d, 2.0 * d, sw, sh, *color, own,
                    ));
                } else {
                    verts.extend(circle_vertices(*cx, *cy, *radius, sw, sh, *color, segs(*radius), no));
                }
            }
            Prim::Sphere { cx, cy, radius, material } if shader_plates => {
                let color = material.fill(PlateRole::Nested);
                let mat = material.finish.to_array();
                // A hemisphere lit per pixel by the plate branch (mode 5): one
                // cover quad, its own never-merged batch. The quad overhangs
                // the disc by 1px for the shader's silhouette anti-aliasing.
                let d = *radius + 1.0;
                verts.extend(quad_vertices(cx - d, cy - d, 2.0 * d, 2.0 * d, sw, sh, color));
                plate = Some(crate::vk::PlatePush {
                    // Center + radius in physical px; the SDF box machinery is
                    // unused in this mode, so .w is free.
                    rect: [cx * scale, cy * scale, radius * scale, 0.0],
                    radii: [0.0; 4],
                    light: [plate_light[0], plate_light[1], plate_light[2], 0.0],
                    material: mat,
                    host: [0.0; 4],
                    specular_tint: [1.0, 1.0, 1.0, 0.0],
                    mode: 5.0,
                    shape: 2.0,
                });
            }
            Prim::Sphere { cx, cy, radius, material } => {
                let color = material.fill(PlateRole::Nested);
                // Legacy path: the flat disc, exactly a Circle.
                verts.extend(circle_vertices(*cx, *cy, *radius, sw, sh, color, segs(*radius), no));
            }
            Prim::DropletScrim { rect, material, spec, feather } if shader_plates => {
                let color = material.fill(PlateRole::Nested);
                let mat = material.finish.to_array();
                // Shader mode 12: the droplet's own SDF, filled flat and
                // feathered inward. No contact shadow, so unlike the lit drop
                // the cover quad is exactly the box — a scrim never draws
                // outside the silhouette.
                let g = droplet_geom(rect, spec);
                verts.extend(quad_vertices(rect.x, rect.y, rect.width, rect.height, sw, sh, color));
                plate = Some(crate::vk::PlatePush {
                    rect: [
                        (rect.x + rect.width * 0.5) * scale,
                        (rect.y + rect.height * 0.5) * scale,
                        g.hx * scale,
                        g.hy * scale,
                    ],
                    radii: [g.sag * scale, g.br * scale, g.bw * scale, g.k * scale],
                    // p_light.w carries the FEATHER here; mode 12 returns
                    // before the shading band it otherwise holds is read.
                    light: [plate_light[0], plate_light[1], plate_light[2], feather.max(0.001) * scale],
                    material: [mat[0], 0.0, 0.0, 0.0],
                    host: [g.sr * scale, 0.0, 0.0, g.ar * scale],
                    specular_tint: [0.0, 0.0, 0.0, g.bow * scale],
                    mode: 12.0,
                    shape: spec.curve.clamp(2.0, 6.0),
                });
            }
            Prim::Droplet { rect, material, spec } if shader_plates => {
                let color = material.fill(PlateRole::Nested);
                let mat = material.finish.to_array();
                // A water droplet lit by shader mode 10: one cover quad; the
                // shader owns silhouette (sheet ∪smin belly), dome shading,
                // fresnel rim and thin-edge clarity. The spec's height
                // fractions resolve against the concrete rect here, clamped so
                // small or narrow boxes stay well-formed (a belly wider than
                // the box would turn the SDF interior inside out).
                // The cover quad grows sideways and BELOW the box by the
                // contact shadow's reach — shadow fragments live outside the
                // silhouette, so they need covered pixels to shade.
                let g = droplet_geom(rect, spec);
                let (hx, hy, sag, br, bw, k, sr, ar, band, bow, sh_reach) =
                    (g.hx, g.hy, g.sag, g.br, g.bw, g.k, g.sr, g.ar, g.band, g.bow, g.sh_reach);
                verts.extend(quad_vertices(
                    rect.x - sh_reach,
                    rect.y,
                    rect.width + 2.0 * sh_reach,
                    rect.height + sh_reach,
                    sw, sh, color,
                ));
                plate = Some(crate::vk::PlatePush {
                    rect: [
                        (rect.x + rect.width * 0.5) * scale,
                        (rect.y + rect.height * 0.5) * scale,
                        hx * scale,
                        hy * scale,
                    ],
                    radii: [sag * scale, br * scale, bw * scale, k * scale],
                    light: [plate_light[0], plate_light[1], plate_light[2], band * scale],
                    // Slots y/z/w feed roll_spec and the rim term directly:
                    // a droplet's material carries its own gleam/shine/rim
                    // there (`DropletSpec::finish`; a drop is wetter than the
                    // DE's plates), so this is the material's finish like any
                    // plate's.
                    material: mat,
                    host: [sr * scale, spec.clarity.clamp(0.0, 1.0), spec.dome, ar * scale],
                    // Droplet glints are always white, so the tint RGB slots
                    // carry droplet params instead: x = core density,
                    // y = contact-shadow reach px, z = shadow strength.
                    specular_tint: [
                        spec.core.clamp(0.0, 2.0),
                        sh_reach * scale,
                        spec.shadow.clamp(0.0, 1.0),
                        bow * scale,
                    ],
                    mode: 10.0,
                    shape: spec.curve.clamp(2.0, 6.0),
                });
            }
            Prim::DropletScrim { rect, material, spec, .. } => {
                let color = material.fill(PlateRole::Nested);
                // Legacy banded path: no SDF to feather against, so the scrim
                // degrades to the same flat outline the drop itself does —
                // hard-edged, but present. A prim with no arm here VANISHES.
                let cap = (rect.height * 0.5).min(rect.width * 0.5);
                let sr = (spec.sheet_r.clamp(0.0, 1.0) * rect.height).min(cap);
                let ar = (spec.attach.clamp(0.0, 1.0) * rect.height).min(cap);
                let radii = crate::widget::CornerRadii::new(ar, ar, sr, sr);
                push_rounded_rect_vertices_corners(rect.x, rect.y, rect.width, rect.height, radii, sw, sh, color, no, None, &mut verts);
            }
            Prim::Droplet { rect, material, spec } => {
                let color = material.fill(PlateRole::Nested);
                // Legacy banded path: the flat drop outline — attach-tapered
                // top, round bottom. Degrades the material but keeps the
                // silhouette (a prim with no arm here VANISHES, it doesn't
                // degrade — see Ridge/Groove above).
                let cap = (rect.height * 0.5).min(rect.width * 0.5);
                let sr = (spec.sheet_r.clamp(0.0, 1.0) * rect.height).min(cap);
                let ar = (spec.attach.clamp(0.0, 1.0) * rect.height).min(cap);
                let radii = crate::widget::CornerRadii::new(ar, ar, sr, sr);
                push_rounded_rect_vertices_corners(rect.x, rect.y, rect.width, rect.height, radii, sw, sh, color, no, None, &mut verts);
            }
            Prim::ConcaveFillet { cx, cy, radius, depth, start: a0, raised } if shader_plates => {
                // A quarter-arc carve wall (shader mode 6/7): one cover quad
                // over the wedge's reach; the wall straddles the arc by ±t/2
                // like every carve boundary. p_rect carries centre + radius,
                // p_radii.x the wedge start angle. Host box pushed far out —
                // an inside-corner fillet never fades.
                let m = *depth * 0.5 + 2.0;
                let r = *radius + m;
                verts.extend(quad_vertices(cx - r, cy - r, 2.0 * r, 2.0 * r, sw, sh, [0.0; 4]));
                plate = Some(crate::vk::PlatePush {
                    rect: [cx * scale, cy * scale, *radius * scale, 0.0],
                    radii: [*a0, 0.0, 0.0, 0.0],
                    light: [plate_light[0], plate_light[1], plate_light[2], *depth * scale],
                    material: plate_mat,
                    host: [0.0, 0.0, 1e6, 1e6],
                    specular_tint: [1.0, 1.0, 1.0, 0.0],
                    mode: if *raised { 7.0 } else { 6.0 },
                    shape: crate::layout::corner_shape(),
                });
            }
            // Legacy banded path has no radial wall — the composed corner
            // stays square there (A/B comparison path only).
            Prim::ConcaveFillet { .. } => {}
            Prim::Groove { a, b, width, depth, host } if shader_plates => {
                // A slab carve about the line a–b (shader mode 8): the cover
                // quad is the segment's bounding box grown by the groove's own
                // half-width plus the wall's reach. Off-band corners of that
                // box sit at u = 1 (plateau), so the box overhang shades
                // nothing — the slab is what bounds the mark, not the quad.
                let m = *width * 0.5 + *depth * 0.5 + 2.0;
                let (x0, x1) = (a.0.min(b.0) - m, a.0.max(b.0) + m);
                let (y0, y1) = (a.1.min(b.1) - m, a.1.max(b.1) + m);
                verts.extend(quad_vertices(x0, y0, x1 - x0, y1 - y0, sw, sh, [0.0; 4]));
                // Unit normal of the line — the direction the slab's distance is
                // measured along. A degenerate segment falls back to vertical so
                // a zero-length groove is a no-op wall rather than a NaN.
                let (dx, dy) = (b.0 - a.0, b.1 - a.1);
                let len = (dx * dx + dy * dy).sqrt();
                let n = if len > 1e-4 { (-dy / len, dx / len) } else { (1.0, 0.0) };
                plate = Some(crate::vk::PlatePush {
                    // Centre + slab half-width in physical px; .w unused.
                    rect: [
                        (a.0 + b.0) * 0.5 * scale,
                        (a.1 + b.1) * 0.5 * scale,
                        *width * 0.5 * scale,
                        0.0,
                    ],
                    radii: [n.0, n.1, 0.0, 0.0],
                    light: [plate_light[0], plate_light[1], plate_light[2], *depth * scale],
                    material: plate_mat,
                    host: [
                        (host.x + host.width * 0.5) * scale,
                        (host.y + host.height * 0.5) * scale,
                        host.width * 0.5 * scale,
                        host.height * 0.5 * scale,
                    ],
                    specular_tint: [1.0, 1.0, 1.0, 0.0],
                    mode: 8.0,
                    shape: crate::layout::corner_shape(),
                });
            }
            Prim::Groove { a, b, width, depth, host: _ } => {
                // Legacy approximation. The banded tessellators walk BOX edges —
                // exactly the axis-aligned assumption a groove exists to escape —
                // so the walls are drawn directly as two feathered lines meeting
                // at the centerline: the engraved-line fake, one half in shadow
                // and one lit. Coarser than the SDF (no profile curve, no host
                // fade), but this path exists for A/B comparison, and drawing
                // NOTHING would silently delete the mark rather than degrade it
                // — see `Prim::Ridge` above, which accepts a hot crest for the
                // same reason.
                let (dx, dy) = (b.0 - a.0, b.1 - a.1);
                let len = (dx * dx + dy * dy).sqrt();
                if len < 0.001 {
                    continue;
                }
                let n = (-dy / len, dx / len);
                // Same convention as `push_bevel_edge_vertices_banded`: the
                // light folded through `light_sign` (-1.0 — a groove is a
                // carve), dotted with each wall's OUTWARD normal, amplitude on
                // `bevel_depth`. So a groove re-lights with the DE's light
                // instead of hardcoding which side is dark.
                let rad = crate::layout::light_source_position();
                let (lx, ly) = (-rad.cos(), rad.sin());
                let v = crate::layout::bevel_depth() * (n.0 * lx + n.1 * ly);
                // Each wall covers its own half, centreline to outer edge —
                // abutting rather than overlapping. The SDF gets away with
                // walls that overlap across a sub-pixel floor because it is one
                // evaluation of |distance|; two opposite-signed overlays would
                // just blend to mud.
                let half = (*width * 0.5 + *depth * 0.5).max(0.5);
                for side in [1.0f32, -1.0] {
                    let sv = v * side;
                    let c = if sv >= 0.0 { overlay_light(sv) } else { overlay_dark(sv) };
                    if c[3] <= 0.0 {
                        continue;
                    }
                    let off = side * half * 0.5;
                    push_feathered_line_vertices(
                        a.0 + n.0 * off, a.1 + n.1 * off,
                        b.0 + n.0 * off, b.1 + n.1 * off,
                        half, sw, sh, c, &mut verts,
                    );
                }
            }
            Prim::Lattice { rect, period, origin, cell, radius, depth } if shader_plates => {
                // A periodic well field (shader mode 13): one cover quad over
                // `rect`; the shader folds each pixel into the period and
                // measures the nearest cell, so the whole lattice is a single
                // evaluation. p_rect = one cell's centre + half-extents,
                // p_radii = the corner radius, p_host.xy = the period; the
                // host-box fade sides are pushed far out (a lattice never
                // fades against a host — its own rect bounds it).
                let (pw, ph) = (period.0.max(1e-3), period.1.max(1e-3));
                verts.extend(quad_vertices(rect.x, rect.y, rect.width, rect.height, sw, sh, [0.0; 4]));
                plate = Some(crate::vk::PlatePush {
                    rect: [origin.0 * scale, origin.1 * scale, cell.0 * 0.5 * scale, cell.1 * 0.5 * scale],
                    radii: [*radius * scale; 4],
                    light: [plate_light[0], plate_light[1], plate_light[2], *depth * scale],
                    material: plate_mat,
                    host: [pw * scale, ph * scale, 1e6, 1e6],
                    specular_tint: [1.0, 1.0, 1.0, 0.0],
                    mode: 13.0,
                    shape: crate::layout::corner_shape(),
                });
            }
            // Legacy banded path: no periodic wall — the lattice draws nothing
            // there, like the fillet (A/B comparison path only).
            Prim::Lattice { .. } => {}
            Prim::CarveUnion { boxes, depth, raised } if shader_plates => {
                // The union of several boxes as ONE wall (shader mode 14): the
                // boxes go into the frame's feature buffer as a contiguous run
                // and the shader takes the nearest one per pixel. The cover
                // quad is the union's bounding box grown by the wall's reach;
                // off-shape corners of it sit at the plateau and shade nothing.
                let budget = crate::vk::MAX_PLATE_FEATURES.saturating_sub(features.len());
                let take = boxes.len().min(budget);
                if take < boxes.len() && plate_debug() {
                    eprintln!(
                        "plate-carve: union of {} boxes gets {} — feature budget full ({} used)",
                        boxes.len(), take, features.len()
                    );
                }
                if take == 0 {
                    continue;
                }
                let kept = &boxes[..take];
                let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
                for (r, _) in kept {
                    x0 = x0.min(r.x);
                    y0 = y0.min(r.y);
                    x1 = x1.max(r.x + r.width);
                    y1 = y1.max(r.y + r.height);
                }
                let infl = *depth * 0.5 + 2.0;
                verts.extend(quad_vertices(
                    x0 - infl, y0 - infl,
                    (x1 - x0) + 2.0 * infl, (y1 - y0) + 2.0 * infl,
                    sw, sh, [0.0; 4],
                ));
                let off = features.len() as f32;
                for (r, radii) in kept {
                    features.push([
                        (r.x + r.width * 0.5) * scale,
                        (r.y + r.height * 0.5) * scale,
                        r.width * 0.5 * scale,
                        r.height * 0.5 * scale,
                        radii.0 * scale,
                        radii.1 * scale,
                        radii.2 * scale,
                        radii.3 * scale,
                        *depth * scale,
                        0.0,
                        0.0,
                        0.0,
                    ]);
                }
                // The run is complete: a plate with an open feature run must
                // not append past it (its features would no longer be
                // contiguous), so it is closed here like any other appender.
                last_feature_plate = None;
                plate = Some(crate::vk::PlatePush {
                    rect: [
                        (x0 + x1) * 0.5 * scale,
                        (y0 + y1) * 0.5 * scale,
                        (x1 - x0) * 0.5 * scale,
                        (y1 - y0) * 0.5 * scale,
                    ],
                    // x: the raised flag; the shader reads nothing else here.
                    radii: [if *raised { 1.0 } else { 0.0 }, 0.0, 0.0, 0.0],
                    light: [plate_light[0], plate_light[1], plate_light[2], *depth * scale],
                    material: plate_mat,
                    // Feature run [offset, count] (the renderer rebases the
                    // offset onto the frame slot, as for mode 1); zw far out
                    // so the host-box fade never applies.
                    host: [off, take as f32, 1e6, 1e6],
                    specular_tint: [1.0, 1.0, 1.0, 0.0],
                    mode: 14.0,
                    shape: crate::layout::corner_shape(),
                });
            }
            // Legacy banded path: no union — nothing is drawn there, like the
            // fillet and the lattice (A/B comparison path only).
            Prim::CarveUnion { .. } => {}
        }
        let end = verts.len() as u32;
        if end == start {
            continue;
        }
        // Some tessellators (quad_vertices, vector_vertices) don't thread the circle clip —
        // stamp the whole emitted range so every prim kind honors it uniformly.
        if item.clip_circle.is_some() {
            for v in verts[start as usize..].iter_mut() {
                v.clip_circle = no;
            }
        }
        // Merge into the previous batch if it shares this clip pair and is contiguous.
        // Plate batches carry per-draw push constants, and blur-behind batches
        // trigger the renderer's snapshot copy, so neither ever merges.
        if plate.is_none() && !blur_behind {
            // Ordinary geometry painted after a plate ends its carve-grouping
            // window: a recess emitted later must overlay this geometry (the
            // fallback path), not shade beneath it inside the plate's draw.
            if dbg_plates && !plate_stack.is_empty() {
                *dbg_closed_by.entry(prim_kind(&item.prim)).or_insert(0) += plate_stack.len();
            }
            plate_stack.clear();
            if let Some(last) = batches.last_mut() {
                if last.plate.is_none()
                    && last.scissor == item.clip
                    && last.clip_rrect == item.clip_rrect
                    && last.end == start
                {
                    last.end = end;
                    continue;
                }
            }
        }
        if promoted {
            plate_stack.clear();
        }
        batches.push(DlBatch { scissor: item.clip, clip_rrect: item.clip_rrect, start, end, plate, blur_behind });
        if let Some(prect) = made_plate {
            plate_stack.push((batches.len() - 1, prect));
            if dbg_plates {
                dbg_opened += 1;
            }
        }
    }

    if dbg_plates && (dbg_grouped > 0 || !dbg_fell_back.is_empty()) {
        eprintln!(
            "plate-dbg: {} carves — {dbg_grouped} grouped (exact CSG), {} overlay fallback",
            dbg_grouped + dbg_fell_back.len(),
            dbg_fell_back.len(),
        );
        eprintln!(
            "plate-dbg:   {dbg_opened} grouping window(s) opened by a filled plate; closed early by {}",
            if dbg_closed_by.is_empty() {
                "nothing".to_string()
            } else {
                dbg_closed_by
                    .iter()
                    .map(|(k, n)| format!("{k}x{n}"))
                    .collect::<Vec<_>>()
                    .join(", ")
            }
        );
        for line in &dbg_fell_back {
            eprintln!("plate-dbg: {line}");
        }
    }

    (verts, batches, images, features)
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
/// circular corners at the nominal radii, and the fill's own frost recipe in
/// `host.zw` (`Material::from_fill` decodes the sentinel).
fn flat_frost_push(
    rect: &crate::scene::layout::Rect,
    radii: (f32, f32, f32, f32),
    fill: [f32; 4],
    scale: f32,
    light: [f32; 3],
    material: [f32; 4],
) -> crate::vk::PlatePush {
    let mut p = plate_push_raised(rect, radii, 0.0, scale, light, material, false, Some(2.0));
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
) -> crate::vk::PlatePush {
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
    crate::vk::PlatePush {
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

pub fn extra_quad_vertices(
    w: &dyn crate::widget::WidgetHost,
    qx: f32, qy: f32, qw: f32, qh: f32,
    sw: f32, sh: f32,
    qc: [f32; 4],
    clip_circle: [f32; 3],
) -> Vec<Vertex> {
    let mut verts = Vec::new();
    push_extra_quad_vertices(w, qx, qy, qw, qh, sw, sh, qc, clip_circle, &mut verts);
    verts
}

fn get_child_widget_for_quad<'a>(
    w: &'a dyn crate::widget::WidgetHost,
    qx: f32, qy: f32, qw: f32, qh: f32,
) -> &'a dyn crate::widget::WidgetHost {
    if let Some(pbg) = w.as_any().downcast_ref::<crate::widget::ParametersBg>() {
        for s_opt in &pbg.sliders {
            if let Some(s) = s_opt {
                let (sx, sy, sww, shh) = s.rect();
                if qx >= sx - 0.1 && qx + qw <= sx + sww + 0.1 && qy >= sy - 0.1 && qy + qh <= sy + shh + 0.1 {
                    return s;
                }
            }
        }
        for f_opt in &pbg.float3s {
            if let Some(f) = f_opt {
                let (fx, fy, fww, fhh) = f.rect();
                if qx >= fx - 0.1 && qx + qw <= fx + fww + 0.1 && qy >= fy - 0.1 && qy + qh <= fy + fhh + 0.1 {
                    return f;
                }
            }
        }
        for sb_opt in &pbg.spinboxes {
            if let Some(sb) = sb_opt {
                let (sx, sy, sww, shh) = sb.rect();
                if qx >= sx - 0.1 && qx + qw <= sx + sww + 0.1 && qy >= sy - 0.1 && qy + qh <= sy + shh + 0.1 {
                    return sb;
                }
            }
        }
        for btn_opt in &pbg.buttons {
            if let Some(btn) = btn_opt {
                let (bx, by, bww, bhh) = btn.rect();
                if qx >= bx - 0.1 && qx + qw <= bx + bww + 0.1 && qy >= by - 0.1 && qy + qh <= by + bhh + 0.1 {
                    return btn;
                }
            }
        }
        for ch_opt in &pbg.choices {
            if let Some(ch) = ch_opt {
                let (cx, cy, cww, chh) = ch.rect();
                if qx >= cx - 0.1 && qx + qw <= cx + cww + 0.1 && qy >= cy - 0.1 && qy + qh <= cy + chh + 0.1 {
                    return ch;
                }
            }
        }
        for t_opt in &pbg.texts {
            if let Some(t) = t_opt {
                let (tx, ty, tww, thh) = t.rect();
                if qx >= tx - 0.1 && qx + qw <= tx + tww + 0.1 && qy >= ty - 0.1 && qy + qh <= ty + thh + 0.1 {
                    return t;
                }
            }
        }
        for cb_opt in &pbg.toggles {
            if let Some(cb) = cb_opt {
                let (cx, cy, cww, chh) = cb.rect();
                if qx >= cx - 0.1 && qx + qw <= cx + cww + 0.1 && qy >= cy - 0.1 && qy + qh <= cy + chh + 0.1 {
                    return cb;
                }
            }
        }
        for c_opt in &pbg.colors {
            if let Some(c) = c_opt {
                let (cx, cy, cww, chh) = c.rect();
                if qx >= cx - 0.1 && qx + qw <= cx + cww + 0.1 && qy >= cy - 0.1 && qy + qh <= cy + chh + 0.1 {
                    return c;
                }
            }
        }
    }
    w
}

pub fn push_extra_quad_vertices(
    w: &dyn crate::widget::WidgetHost,
    qx: f32, qy: f32, qw: f32, qh: f32,
    sw: f32, sh: f32,
    qc: [f32; 4],
    clip_circle: [f32; 3],
    out: &mut Vec<Vertex>,
) {
    if let Some(graph) = w.as_any().downcast_ref::<crate::widget::display::Graph>() {
        if graph.is_node_rect(qx, qy, qw, qh) {
            let r = crate::layout::graph_node_corner_radius();
            let extra_radii = crate::widget::CornerRadii::new(r, r, r, r);
            push_rounded_rect_vertices_corners(qx, qy, qw, qh, extra_radii, sw, sh, qc, clip_circle, None, out);
            return;
        }
    }

    let target_w = get_child_widget_for_quad(w, qx, qy, qw, qh);
    let radii = target_w.corner_radii();
    if radii.top_left <= 0.1 && radii.top_right <= 0.1 && radii.bottom_right <= 0.1 && radii.bottom_left <= 0.1 {
        out.extend_from_slice(&quad_vertices_with_clip(qx, qy, qw, qh, sw, sh, qc, clip_circle));
        if let Some((color, thickness)) = target_w.solid_border() {
            let (wx, wy, ww, wh) = target_w.rect();
            if (qx - wx).abs() < 0.1 && (qy - wy).abs() < 0.1 && (qw - ww).abs() < 0.1 && (qh - wh).abs() < 0.1 {
                push_plate_solid_border_vertices(qx, qy, qw, qh, radii, thickness, sw, sh, color, clip_circle, out);
            }
        }
        return;
    }

    let (wx, mut wy, ww, mut wh) = target_w.rect();
    let top_room = target_w.label_strip();
    wy += top_room;
    wh -= top_room;
    let extra_radii = crate::widget::CornerRadii::new(
        if qx <= wx + 1.5 && qy <= wy + 1.5 { radii.top_left } else { 0.0 },
        if qx + qw >= wx + ww - 1.5 && qy <= wy + 1.5 { radii.top_right } else { 0.0 },
        if qx + qw >= wx + ww - 1.5 && qy + qh >= wy + wh - 1.5 { radii.bottom_right } else { 0.0 },
        if qx <= wx + 1.5 && qy + qh >= wy + wh - 1.5 { radii.bottom_left } else { 0.0 },
    );

    push_rounded_rect_vertices_corners(qx, qy, qw, qh, extra_radii, sw, sh, qc, clip_circle, None, out);

    if let Some((color, thickness)) = target_w.solid_border() {
        let (rx, mut ry, rw, mut rh) = target_w.rect();
        let top = target_w.label_strip();
        ry += top;
        rh -= top;
        if (qx - rx).abs() < 0.1 && (qy - ry).abs() < 0.1 && (qw - rw).abs() < 0.1 && (qh - rh).abs() < 0.1 {
            push_plate_solid_border_vertices(qx, qy, qw, qh, radii, thickness, sw, sh, color, clip_circle, out);
        }
    }
}

pub fn extra_quad_vertices_clipped(
    w: &dyn crate::widget::WidgetHost,
    qx: f32, qy: f32, qw: f32, qh: f32,
    sw: f32, sh: f32,
    qc: [f32; 4],
    clip: (f32, f32, f32, f32),
    clip_circle: [f32; 3],
) -> Vec<Vertex> {
    let mut verts = Vec::new();
    push_extra_quad_vertices_clipped(w, qx, qy, qw, qh, sw, sh, qc, clip, clip_circle, &mut verts);
    verts
}

pub fn push_extra_quad_vertices_clipped(
    w: &dyn crate::widget::WidgetHost,
    qx: f32, qy: f32, qw: f32, qh: f32,
    sw: f32, sh: f32,
    qc: [f32; 4],
    clip: (f32, f32, f32, f32),
    clip_circle: [f32; 3],
    out: &mut Vec<Vertex>,
) {
    if let Some(graph) = w.as_any().downcast_ref::<crate::widget::display::Graph>() {
        if graph.is_node_rect(qx, qy, qw, qh) {
            let r = crate::layout::graph_node_corner_radius();
            let extra_radii = crate::widget::CornerRadii::new(r, r, r, r);
            push_rounded_rect_vertices_corners(qx, qy, qw, qh, extra_radii, sw, sh, qc, clip_circle, Some(clip), out);
            return;
        }
    }

    let target_w = get_child_widget_for_quad(w, qx, qy, qw, qh);
    let radii = target_w.corner_radii();
    if radii.top_left <= 0.1 && radii.top_right <= 0.1 && radii.bottom_right <= 0.1 && radii.bottom_left <= 0.1 {
        let (cx0, cy0, cx1, cy1) = clip;
        let ix0 = qx.max(cx0);
        let iy0 = qy.max(cy0);
        let ix1 = (qx + qw).min(cx1);
        let iy1 = (qy + qh).min(cy1);
        if ix1 <= ix0 || iy1 <= iy0 {
            return;
        }
        out.extend_from_slice(&quad_vertices_with_clip(ix0, iy0, ix1 - ix0, iy1 - iy0, sw, sh, qc, clip_circle));
        if let Some((color, thickness)) = target_w.solid_border() {
            let (wx, wy, ww, wh) = target_w.rect();
            if (qx - wx).abs() < 0.1 && (qy - wy).abs() < 0.1 && (qw - ww).abs() < 0.1 && (qh - wh).abs() < 0.1 {
                push_plate_solid_border_vertices(qx, qy, qw, qh, radii, thickness, sw, sh, color, clip_circle, out);
            }
        }
        return;
    }

    let (wx, mut wy, ww, mut wh) = target_w.rect();
    let top_room = target_w.label_strip();
    wy += top_room;
    wh -= top_room;
    let extra_radii = crate::widget::CornerRadii::new(
        if qx <= wx + 1.5 && qy <= wy + 1.5 { radii.top_left } else { 0.0 },
        if qx + qw >= wx + ww - 1.5 && qy <= wy + 1.5 { radii.top_right } else { 0.0 },
        if qx + qw >= wx + ww - 1.5 && qy + qh >= wy + wh - 1.5 { radii.bottom_right } else { 0.0 },
        if qx <= wx + 1.5 && qy + qh >= wy + wh - 1.5 { radii.bottom_left } else { 0.0 },
    );

    push_rounded_rect_vertices_corners(qx, qy, qw, qh, extra_radii, sw, sh, qc, clip_circle, Some(clip), out);

    if let Some((color, thickness)) = target_w.solid_border() {
        let (rx, mut ry, rw, mut rh) = target_w.rect();
        let top = target_w.label_strip();
        ry += top;
        rh -= top;
        if (qx - rx).abs() < 0.1 && (qy - ry).abs() < 0.1 && (qw - rw).abs() < 0.1 && (qh - rh).abs() < 0.1 {
            push_plate_solid_border_vertices(qx, qy, qw, qh, radii, thickness, sw, sh, color, clip_circle, out);
        }
    }
}

pub fn circle_vertices(
    cx: f32, cy: f32, r: f32,
    sw: f32, sh: f32,
    color: [f32; 4],
    segments: usize,
    clip_circle: [f32; 3],
) -> Vec<Vertex> {
    let mut verts = Vec::new();
    for i in 0..segments {
        let theta1 = (i as f32) * 2.0 * std::f32::consts::PI / (segments as f32);
        let theta2 = ((i + 1) as f32) * 2.0 * std::f32::consts::PI / (segments as f32);
        let x0 = cx;
        let y0 = cy;
        let x1 = cx + r * theta1.cos();
        let y1 = cy + r * theta1.sin();
        let x2 = cx + r * theta2.cos();
        let y2 = cy + r * theta2.sin();
        
        let ndc_x0 = (x0 / sw) * 2.0 - 1.0;
        let ndc_y0 = 1.0 - (y0 / sh) * 2.0;
        let ndc_x1 = (x1 / sw) * 2.0 - 1.0;
        let ndc_y1 = 1.0 - (y1 / sh) * 2.0;
        let ndc_x2 = (x2 / sw) * 2.0 - 1.0;
        let ndc_y2 = 1.0 - (y2 / sh) * 2.0;
        
        verts.push(Vertex { position: [ndc_x0, ndc_y0], color, clip_circle });
        verts.push(Vertex { position: [ndc_x1, ndc_y1], color, clip_circle });
        verts.push(Vertex { position: [ndc_x2, ndc_y2], color, clip_circle });
    }
    verts
}

pub fn circle_border_vertices(
    cx: f32, cy: f32, r: f32,
    thickness: f32,
    sw: f32, sh: f32,
    color: [f32; 4],
    segments: usize,
    clip_circle: [f32; 3],
) -> Vec<Vertex> {
    let mut verts = Vec::new();
    for i in 0..segments {
        let theta1 = (i as f32) * 2.0 * std::f32::consts::PI / (segments as f32);
        let theta2 = ((i + 1) as f32) * 2.0 * std::f32::consts::PI / (segments as f32);
        
        let x0 = cx + (r - thickness) * theta1.cos();
        let y0 = cy + (r - thickness) * theta1.sin();
        let x1 = cx + r * theta1.cos();
        let y1 = cy + r * theta1.sin();
        
        let x2 = cx + r * theta2.cos();
        let y2 = cy + r * theta2.sin();
        let x3 = cx + (r - thickness) * theta2.cos();
        let y3 = cy + (r - thickness) * theta2.sin();
        
        let ndc_x0 = (x0 / sw) * 2.0 - 1.0; let ndc_y0 = 1.0 - (y0 / sh) * 2.0;
        let ndc_x1 = (x1 / sw) * 2.0 - 1.0; let ndc_y1 = 1.0 - (y1 / sh) * 2.0;
        let ndc_x2 = (x2 / sw) * 2.0 - 1.0; let ndc_y2 = 1.0 - (y2 / sh) * 2.0;
        let ndc_x3 = (x3 / sw) * 2.0 - 1.0; let ndc_y3 = 1.0 - (y3 / sh) * 2.0;
        
        verts.push(Vertex { position: [ndc_x0, ndc_y0], color, clip_circle });
        verts.push(Vertex { position: [ndc_x1, ndc_y1], color, clip_circle });
        verts.push(Vertex { position: [ndc_x2, ndc_y2], color, clip_circle });
        
        verts.push(Vertex { position: [ndc_x0, ndc_y0], color, clip_circle });
        verts.push(Vertex { position: [ndc_x2, ndc_y2], color, clip_circle });
        verts.push(Vertex { position: [ndc_x3, ndc_y3], color, clip_circle });
    }
    verts
}

pub fn arc_background_vertices(
    cx: f32, cy: f32, r: f32,
    thickness: f32,
    start_angle: f32, end_angle: f32,
    sw: f32, sh: f32,
    color: [f32; 4],
    segments: usize,
    clip_circle: [f32; 3],
) -> Vec<Vertex> {
    let mut verts = Vec::new();
    push_arc_background_vertices(cx, cy, r, thickness, start_angle, end_angle, sw, sh, color, segments, clip_circle, &mut verts);
    verts
}

/// A ring band with radial Gouraud shading: two sub-bands (inner rim → crest
/// centerline, crest → outer rim) whose vertex colors interpolate across the
/// stroke — the rounded-bevel profile — plus the half-px alpha feathers at
/// both true rims (colors matched to the adjacent band, so no seams).
#[allow(clippy::too_many_arguments)]
pub fn push_arc_shaded_vertices(
    cx: f32, cy: f32, r: f32,
    thickness: f32,
    start_angle: f32, end_angle: f32,
    sw: f32, sh: f32,
    inner: [f32; 4], crest: [f32; 4], outer: [f32; 4],
    segments: usize,
    clip_circle: [f32; 3],
    out: &mut Vec<Vertex>,
) {
    let f = 0.5f32.min(thickness * 0.25);
    let r_out = r;
    let r_in = (r - thickness).max(0.0);
    let r_mid = (r_in + r_out) / 2.0;
    let fade_in = [inner[0], inner[1], inner[2], 0.0];
    let fade_out = [outer[0], outer[1], outer[2], 0.0];
    // (inner radius, outer radius, color at inner edge, color at outer edge)
    let bands = [
        ((r_in - f).max(0.0), r_in + f, fade_in, inner),
        (r_in + f, r_mid, inner, crest),
        (r_mid, r_out - f, crest, outer),
        (r_out - f, r_out + f, outer, fade_out),
    ];
    for i in 0..segments {
        let theta1 = start_angle + (i as f32) * (end_angle - start_angle) / (segments as f32);
        let theta2 = start_angle + ((i + 1) as f32) * (end_angle - start_angle) / (segments as f32);
        let (c1, s1) = (theta1.cos(), theta1.sin());
        let (c2, s2) = (theta2.cos(), theta2.sin());
        for &(ra, rb, ca, cb) in &bands {
            if rb <= ra {
                continue;
            }
            let p = |rad: f32, c: f32, s: f32| -> [f32; 2] {
                [((cx + rad * c) / sw) * 2.0 - 1.0, 1.0 - ((cy + rad * s) / sh) * 2.0]
            };
            let (i1, o1) = (p(ra, c1, s1), p(rb, c1, s1));
            let (i2, o2) = (p(ra, c2, s2), p(rb, c2, s2));
            out.push(Vertex { position: i1, color: ca, clip_circle });
            out.push(Vertex { position: o1, color: cb, clip_circle });
            out.push(Vertex { position: o2, color: cb, clip_circle });
            out.push(Vertex { position: i1, color: ca, clip_circle });
            out.push(Vertex { position: o2, color: cb, clip_circle });
            out.push(Vertex { position: i2, color: ca, clip_circle });
        }
    }
}

pub fn push_arc_background_vertices(
    cx: f32, cy: f32, r: f32,
    thickness: f32,
    start_angle: f32, end_angle: f32,
    sw: f32, sh: f32,
    color: [f32; 4],
    segments: usize,
    clip_circle: [f32; 3],
    out: &mut Vec<Vertex>,
) {
    // The stroke band [r - thickness, r], with a half-px alpha ramp on each rim
    // (Gouraud across thin edge bands) so curved edges resolve smoothly instead
    // of hard-stepping — the poor-man's AA the flat pipeline doesn't provide.
    let f = 0.5f32.min(thickness * 0.25);
    let r_in = (r - thickness).max(0.0);
    // (inner radius, outer radius, alpha at inner rim, alpha at outer rim)
    let bands = [
        ((r_in - f).max(0.0), r_in + f, 0.0, color[3]),
        (r_in + f, r - f, color[3], color[3]),
        (r - f, r + f, color[3], 0.0),
    ];
    for i in 0..segments {
        let theta1 = start_angle + (i as f32) * (end_angle - start_angle) / (segments as f32);
        let theta2 = start_angle + ((i + 1) as f32) * (end_angle - start_angle) / (segments as f32);
        let (c1, s1) = (theta1.cos(), theta1.sin());
        let (c2, s2) = (theta2.cos(), theta2.sin());
        for &(ra, rb, aa, ab) in &bands {
            if rb <= ra {
                continue;
            }
            let ca = [color[0], color[1], color[2], aa];
            let cb = [color[0], color[1], color[2], ab];
            let p = |rad: f32, c: f32, s: f32| -> [f32; 2] {
                [((cx + rad * c) / sw) * 2.0 - 1.0, 1.0 - ((cy + rad * s) / sh) * 2.0]
            };
            let (i1, o1) = (p(ra, c1, s1), p(rb, c1, s1));
            let (i2, o2) = (p(ra, c2, s2), p(rb, c2, s2));
            out.push(Vertex { position: i1, color: ca, clip_circle });
            out.push(Vertex { position: o1, color: cb, clip_circle });
            out.push(Vertex { position: o2, color: cb, clip_circle });
            out.push(Vertex { position: i1, color: ca, clip_circle });
            out.push(Vertex { position: o2, color: cb, clip_circle });
            out.push(Vertex { position: i2, color: ca, clip_circle });
        }
    }
}

#[derive(Debug, Clone)]
pub struct WindowSettings {
    pub title: String,
    pub app_id: String,
    pub width: u32,
    pub height: u32,
    pub fullscreen: bool,
    pub min_size: Option<(u32, u32)>,
}

/// A compositor-side window operation requested by the app: an interactive
/// move or resize grab. Returned from [`Application::take_window_action`];
/// the runner executes it with the serial of the most recent pointer press.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowAction {
    Move,
    Resize(xdg_toplevel::ResizeEdge),
}

// Re-export the wlr-layer-shell types apps need to describe a layer surface.
pub use smithay_client_toolkit::shell::wlr_layer::{
    Anchor as LayerAnchor, KeyboardInteractivity as LayerKeyboardInteractivity, Layer as LayerKind,
};

/// Opt-in configuration for running an [`Application`] on a wlr-layer-shell
/// surface (panels, overlays, notifications) instead of an xdg toplevel.
/// Return one from [`Application::layer`] to select layer-shell.
#[derive(Debug, Clone)]
pub struct LayerSettings {
    pub layer: LayerKind,
    pub anchor: LayerAnchor,
    pub exclusive_zone: i32,
    pub keyboard_interactivity: LayerKeyboardInteractivity,
    /// (top, right, bottom, left) margins in logical pixels.
    pub margin: (i32, i32, i32, i32),
    pub namespace: String,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LogicalPosition {
    pub x: f32,
    pub y: f32,
}

impl LogicalPosition {
    pub fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LogicalSize {
    pub width: f32,
    pub height: f32,
}

impl LogicalSize {
    pub fn new(width: f32, height: f32) -> Self {
        Self { width, height }
    }
}

pub struct RenderContext<'a> {
    pub font_system: &'a mut FontSystem,
}

pub trait Application: Sized + 'static {
    type Message: Send + Clone + 'static;

    fn new(qh: &QueueHandle<EngineState<Self>>, sender: calloop::channel::Sender<Self::Message>) -> Self;
    fn settings(&self) -> WindowSettings;
    /// Return `Some(..)` to run on a wlr-layer-shell surface (overlay/panel)
    /// instead of an xdg toplevel. Defaults to `None` (a normal window).
    fn layer(&self) -> Option<LayerSettings> {
        None
    }
    /// Declare the window a UTILITY window: a tool whose shape is decided by
    /// its contents. The compositor then never dictates a size to it (every
    /// configure is the "you choose" 0x0 — [`WindowSettings::width`]/`height`
    /// become the surface's own initial size), offers no resize affordance
    /// (the whole border band moves the window), and never saves geometry
    /// for it, so a stale remembered size can't be restored over what the
    /// app asks for. Declared over the cce window-management protocol at
    /// window creation; on a compositor too old to know the request this is
    /// silently a plain floating window. Defaults to `false`.
    fn utility(&self) -> bool {
        false
    }
    /// Declare the window the DESKTOP-GRID layer (zcce set_grid): the
    /// compositor world-anchors the surface to the virtual desktop and
    /// pans/zooms it per frame like window content; the app renders only
    /// when handed a patch (see [`Application::grid_patch`]). The surface
    /// becomes input-transparent and lives behind all windows. Needs
    /// manager v6; on an older compositor the declaration is skipped.
    /// Defaults to `false`.
    fn grid(&self) -> bool {
        false
    }
    /// A grid patch to render (grid apps only): virtual origin (`x`, `y`),
    /// virtual size (`w`, `h`), and `scale` surface px per virtual unit.
    /// Called right before the frame that must show it; the runner has
    /// already resized the surface to `(w*scale, h*scale)` and acks the
    /// patch so the coming commit is latched at the new anchor.
    fn grid_patch(&mut self, _x: f64, _y: f64, _w: f64, _h: f64, _scale: f64) {}
    fn update(&mut self, msg: Self::Message, needs_rebuild: &mut bool, exit: &mut bool);
    fn tick(&mut self, dt: f32, needs_rebuild: &mut bool);
    /// How long the runner may sleep between `tick`s while the window is
    /// idle — nothing to draw, no animation, no key held, no frame callback
    /// outstanding. `None` (the default) lets it sleep until a Wayland
    /// event or a message on the app's calloop `Sender` arrives, bounded by
    /// [`IDLE_DISPATCH`]. Override with `Some` ONLY if your `tick` polls
    /// something the loop cannot see — a `std::sync::mpsc` receiver drained
    /// in `tick`, say — because with the default that poll waits for the
    /// next unrelated event. The better fix is to send through the calloop
    /// `Sender` handed to `new`, which wakes the loop by itself.
    fn idle_poll_interval(&self) -> Option<std::time::Duration> {
        None
    }
    /// On-top overlay quads drawn after the display list and its text (e.g. the status bar's
    /// tray-hover highlights). Deliberately separate from the single paint path.
    fn overlay_quads(&mut self, _quads: &mut Vec<(f32, f32, f32, f32, [f32; 4])>, _size: LogicalSize, _scale: f64) {}
    fn input_regions(&self) -> Option<Vec<(i32, i32, i32, i32)>> {
        None
    }

    /// Transparent overflow rim, in logical px, on the RIGHT and BOTTOM of
    /// the window. Non-zero opts into buffer-larger-than-geometry mode: the
    /// runner sizes the surface `margin` wider/taller than the configured
    /// window size, publishes the top-left rect as the xdg window geometry
    /// (what the compositor tiles, borders, and snaps) and an input region of
    /// the frame plus any open popover rects — an overhanging menu stays
    /// clickable while empty rim falls through to whatever is behind.
    ///
    /// Right/bottom ONLY, deliberately: the surface grows away from its
    /// origin, so the frame never moves relative to the surface and pointer
    /// coordinates stay valid across the resize (a leading rim shifts the
    /// surface under an unmoved cursor, and the compositor's stale pointer
    /// state then drops the very next click). Frame coords == surface coords:
    /// no input translation, no paint shift — the app's only obligation is to
    /// lay out against the frame (`display_list`'s `size` minus the margin);
    /// content emitted past the frame edge renders in the rim instead of
    /// clipping at the buffer edge.
    ///
    /// The value may change at runtime (return the popover overhang while a
    /// menu is open, 0 otherwise): the engine re-derives the surface from the
    /// stored frame and resizes on drift. Quantize the answer (e.g. 64px
    /// steps) so an animating popover doesn't resize the surface per frame.
    /// xdg toplevels only (layer surfaces ignore it).
    fn overflow_margin(&self) -> u32 {
        0
    }

    fn desired_size(&self) -> Option<(u32, u32)> {
        None
    }
    
    fn ui_context(&self) -> Option<&crate::context::UiContext> {
        None
    }

    fn ui_context_mut(&mut self) -> Option<&mut crate::context::UiContext> {
        None
    }

    /// Whether a left-press at (px, py) should start a compositor window drag. Every root
    /// root plate container is dissolved (Phase 6), so the default is "no" — apps that want
    /// drag-anywhere override this with `ctx.drag_allowed_at(px, py)`.
    fn is_movable_root_plate_at(&self, _px: f32, _py: f32) -> bool {
        false
    }
    
    fn clear_color(&self) -> [f32; 4] {
        [0.0, 0.0, 0.0, 0.0]
    }

    fn register_sources(&mut self, _handle: &calloop::LoopHandle<'_, EngineState<Self>>) {}

    fn adjust_size(&self, width: f32, height: f32) -> (f32, f32) {
        (width, height)
    }
    
    /// Mime types this app accepts from a drag, in the app's own preference
    /// order (the source's order is ignored — a browser lists `text/html`
    /// before `text/uri-list` and which is more useful is the app's call).
    /// The default is empty: the app accepts nothing and drags over it read
    /// as "can't drop here", which is what every client did before drops
    /// existed. Opting in also requires [`Application::handle_drop`].
    fn drop_mimes(&self) -> &'static [&'static str] {
        &[]
    }

    /// A completed drop: `data` is everything the source wrote for `mime`,
    /// and `pos` is where it was released in the app's logical coordinates.
    /// Runs on the main loop, after the transfer finished — this is not the
    /// place to block, since the compositor is waiting on the next frame.
    fn handle_drop(
        &mut self,
        _mime: &str,
        _data: &[u8],
        _pos: LogicalPosition,
        _needs_rebuild: &mut bool,
    ) {
    }

    fn handle_pointer_move(&mut self, pos: LogicalPosition, needs_rebuild: &mut bool);
    fn handle_mouse_input(&mut self, button: MouseButton, state: ElementState, pos: LogicalPosition, needs_rebuild: &mut bool) -> Option<Self::Message>;
    fn handle_mouse_wheel(&mut self, delta: &MouseScrollDelta, pos: LogicalPosition, needs_rebuild: &mut bool);
    /// Trackpad pinch (zwp_pointer_gestures pinch). `factor` is the scale
    /// change SINCE THE LAST update (1.0 = no change, >1 = fingers spreading),
    /// so direct-manipulation zoom is `content_scale *= factor`. Return true
    /// to consume; returning false falls back to the engine's legacy
    /// synthesis — a ctrl+wheel PixelDelta sized for the graph's zoom mapping
    /// (`y = (factor-1)/0.015`) — so ctrl-scroll-zoom surfaces keep working
    /// without implementing this.
    fn handle_pinch(&mut self, _factor: f32, _pos: LogicalPosition, _needs_rebuild: &mut bool) -> bool {
        false
    }
    fn handle_key_input(&mut self, event: &KeyEvent, needs_rebuild: &mut bool) -> Option<Self::Message>;

    /// Undo, after the focused widget declined the chord (a text box that is
    /// editing takes it for its own typing). Return true when something was
    /// undone; false lets the key fall through to `handle_key_input` like any
    /// other. The chords are `undo` / `redo` in `input.kdl` (cce-ui domain
    /// defaults `ctrl+z` / `ctrl+shift+z`), resolved once at startup. Build
    /// the history on `cce_ui::history::History`.
    fn undo(&mut self, _needs_rebuild: &mut bool) -> bool {
        false
    }

    /// Redo — see [`undo`](Self::undo).
    fn redo(&mut self, _needs_rebuild: &mut bool) -> bool {
        false
    }

    /// Opt into the toolkit's keyboard navigation in plate terms: Tab and
    /// Shift+Tab move focus to the next / previous plate or well in reading
    /// order (`UiContext::focus_step`), a press (Enter / Space) acts on the
    /// focused plate, a well opens for typing when focused. Default false: an
    /// app that routes Tab itself (a terminal, a web view, its own field
    /// order) is undisturbed. See "Plates, wells and seams" in `CLAUDE.md`.
    fn plate_navigation(&self) -> bool {
        false
    }

    /// Keyboard focus just moved by the toolkit's Tab traversal. An app that
    /// caches its geometry until its own rebuild flag (relief carves collected
    /// in a view pass, widget lists built on layout) raises that flag here, so
    /// the new ring is drawn; an app that paints fresh every frame needs
    /// nothing. Default: nothing.
    fn focus_stepped(&mut self) {}
    /// Keyboard focus entered/left the window (the compositor keyboard-focuses
    /// the focused window, so this is the "am I the focused window" signal —
    /// e.g. for focus-dependent chrome). Default: ignore.
    fn handle_focus_change(&mut self, _focused: bool, _needs_rebuild: &mut bool) {}

    fn custom_vertices(&mut self, _verts: &mut Vec<Vertex>, _size: LogicalSize, _scale: f64) {}

    /// The frame's geometry, drawn via one batched, GPU-scissor-clipped pass (the single
    /// paint path). Every rendering app implements this — the legacy `view*` sinks are gone;
    /// `None` yields an empty frame. Overlays ([`overlay_quads`](Application::overlay_quads))
    /// and [`custom_vertices`](Application::custom_vertices) still go through their own paths;
    /// text renders from the list when [`display_list_text`](Application::display_list_text)
    /// opts in. Receives the frame's logical size and HiDPI scale. Typically implemented as
    /// `Some(cce_ui::scene::painter::paint_tree(&self.ui_context, &self.root))`.
    fn display_list(&mut self, _size: LogicalSize, _scale: f64) -> Option<crate::scene::paint::DisplayList> {
        None
    }

    /// Opt in to render the display list's `Prim::Text` items through the glyph pass
    /// (shaped via the shared buffer cache, clipped to the item clip ∩ the prim bounds). An
    /// app's ENTIRE frame — geometry and text — is then one
    /// [`display_list`](Application::display_list). Default `false` draws no text (an app that
    /// only draws geometry, or none at all).
    ///
    /// Display-list text gets the same popover-occlusion clamp as the legacy `text_areas`
    /// mapping (`popover_occlusion_clamp`, driven by `ui_context().active_popovers`), so an
    /// open popover's plate clips list text beneath it on both paths.
    fn display_list_text(&self) -> bool {
        false
    }

    /// Opt into system fonts in the ENGINE's render `FontSystem` (the one that shapes
    /// display-list text and rasterizes every glyph at prepare time). Default `false`: the
    /// render FontSystem loads only the bundled CCE fonts, and text asking for a family that
    /// exists only among installed system fonts is silently invisible — buffers shaped
    /// app-side against a system-fonts `FontSystem` carry fontdb face IDs the engine's
    /// database doesn't have (the cce-colors Phase 6e bug). An app whose UI must render
    /// arbitrary installed families (the font picker) returns `true`; its own `FontSystem`,
    /// if it keeps one for measurement, should be `create_font_system_with_system_fonts()`
    /// so both databases load identically. Consulted once, at GPU init.
    fn load_system_fonts(&self) -> bool {
        false
    }

    /// Called once, right after the renderer is created and before the first
    /// frame: create persistent renderer resources here (3D meshes via
    /// [`VkRenderer::create_mesh`]). Most 2D apps never need this.
    fn renderer_init(&mut self, _renderer: &mut VkRenderer) {}

    /// Direct renderer staging, called every frame after the engine's own text
    /// prep and immediately before the frame is drawn: stage 3D scene panes
    /// (`stage_scene`), path-traced panes (`stage_rt`), flush mesh updates, or
    /// prepare app-shaped text (`prepare_text` — an app that returns `false`
    /// from [`display_list_text`](Application::display_list_text) fully owns
    /// the renderer's text state, the engine never touches it). Return `true`
    /// to request another frame immediately (e.g. while a path tracer is still
    /// accumulating samples).
    fn stage_renderer(&mut self, _renderer: &mut VkRenderer, _size: LogicalSize, _scale: f64) -> bool {
        false
    }

    /// The surface was resized (or the scale factor changed): `width`/`height`
    /// are the new logical size. The renderer has already been resized; use
    /// this for stateful relayout that can't wait for the next paint callback.
    fn handle_resize(&mut self, _width: f32, _height: f32, _scale: f64) {}

    /// Whether the runner's built-in client-side decorations apply: the
    /// titlebar move band, the movable-root plate drag regions, and — when
    /// [`csd_resize_borders`](Application::csd_resize_borders) is also on —
    /// the rect-edge resize grabs and their edge cursors. Return `false` for a
    /// window whose chrome doesn't follow its rect (e.g. a circular pane) and
    /// drive moves/resizes yourself via
    /// [`take_window_action`](Application::take_window_action).
    fn standard_csd(&self) -> bool {
        true
    }

    /// Whether the standard CSD claims the outer 8px of the surface as resize
    /// grabs (with matching edge cursors). Off by default: under the cce
    /// compositor the server already provides a resize band just *outside* the
    /// window, so enabling this gives a window two adjacent 8px gutters driven
    /// by different code paths — and only the compositor's snaps to the
    /// desktop grid. It also costs the app clicks, since a press inside the
    /// band starts a grab and never reaches the widgets underneath.
    ///
    /// Turn it on for a window that must be resizable by its own edges under a
    /// compositor that provides no such affordance. Only consulted when
    /// [`standard_csd`](Application::standard_csd) is on.
    fn csd_resize_borders(&self) -> bool {
        false
    }

    /// Whether the standard CSD reserves an implicit title-bar strip (`y` in `[8, 32)`) as a
    /// drag-to-move handle. Opt-in: off by default, so a window has no title bar and is moved
    /// through the compositor (or via explicitly-declared handles —
    /// [`is_movable_root_plate_at`](Application::is_movable_root_plate_at)); nothing is
    /// implicitly draggable. An app with an actual title bar returns `true`. Separate from
    /// [`standard_csd`](Application::standard_csd), which also gates the resize borders, and
    /// only consulted when `standard_csd()` is on.
    fn csd_titlebar_move(&self) -> bool {
        false
    }

    /// Override the pointer cursor at (x, y). `None` falls back to the
    /// runner's standard CSD edge cursors (or `Default` when
    /// [`standard_csd`](Application::standard_csd) is off).
    fn cursor_icon(&self, _x: f32, _y: f32) -> Option<CursorIcon> {
        None
    }

    /// Polled after each pointer frame is dispatched: return a
    /// [`WindowAction`] to start an interactive move/resize grab with the
    /// serial of the most recent pointer press. This is take-semantics — the
    /// implementation should clear its pending action when returning it.
    fn take_window_action(&mut self) -> Option<WindowAction> {
        None
    }

    /// Called once when the event loop ends (window closed, app-requested
    /// exit): last-chance work like autosave. The surface is still alive.
    fn on_exit(&mut self) {}
}

pub struct PressedKey {
    pub logical_key: Key,
    pub text: Option<String>,
    pub first_pressed: Instant,
    pub last_repeated: Instant,
}

fn is_repeatable_key(key: &Key) -> bool {
    match key {
        Key::Named(NamedKey::Backspace) |
        Key::Named(NamedKey::Delete) |
        Key::Named(NamedKey::ArrowLeft) |
        Key::Named(NamedKey::ArrowRight) |
        Key::Named(NamedKey::ArrowUp) |
        Key::Named(NamedKey::ArrowDown) |
        Key::Named(NamedKey::Home) |
        Key::Named(NamedKey::End) |
        Key::Character(_) => true,
        _ => false,
    }
}

/// Default cap on the runner's idle sleep — see `Application::idle_poll_interval`.
pub const IDLE_DISPATCH: std::time::Duration = std::time::Duration::from_millis(1000);

pub struct EngineState<A: Application> {
    pub registry_state: RegistryState,
    pub compositor_state: CompositorState,
    pub xdg_shell_state: XdgShell,
    pub layer_shell_state: Option<LayerShell>,
    pub shm_state: Shm,
    pub seat_state: SeatState,
    pub output_state: OutputState,
    pub seats: Vec<wl_seat::WlSeat>,
    pub pointer: Option<ThemedPointer>,
    pub keyboard: Option<wl_keyboard::WlKeyboard>,

    pub window: Option<XdgWindow>,
    pub layer_surface: Option<LayerSurface>,
    pub surface: Option<wl_surface::WlSurface>,
    
    pub inner: Option<A>,
    
    pub renderer: Option<VkRenderer>,
    pub font_system: Option<FontSystem>,
    pub swash_cache: cosmic_text::SwashCache,
    
    pub scale_factor: f64,
    /// The buffer scale last sent to the surface. Updated in [`Self::render`],
    /// paired with the present that commits a matching-size buffer — never on
    /// the scale event itself, which races in-flight presents of old buffers.
    pub committed_buffer_scale: i32,
    /// Outputs the surface has entered and not left. Used by
    /// `scale_factor_changed` to reject the SCTK no-outputs fallback: on
    /// suspend/resume the DRM connector is destroyed and re-created, the
    /// surface briefly sits on zero (live) outputs, and SCTK reports scale 1.
    /// Acting on that report rebuilds the buffer at scale-1 size while the
    /// surface's latched scale can still be 2 — a fatal `invalid_size`
    /// protocol error for odd-sized surfaces (the status bar crash-loop on
    /// every resume) and a silently HALF-SIZE window for even-sized ones
    /// (the compositor reads buffer/scale as a self-resize and the halving
    /// sticks, compounding per resume).
    pub entered_outputs: Vec<wl_output::WlOutput>,
    pub logical_width: f32,
    pub logical_height: f32,
    /// The window-frame logical size (surface minus the overflow rim) as of
    /// the last configure/desired-size — what the surface is re-derived from
    /// when [`Application::overflow_margin`] changes at runtime.
    pub frame_logical: (f32, f32),
    /// The overflow margin the current surface was actually sized with. Input
    /// translation and the dl-text overlay offsets use THIS, never a live
    /// `overflow_margin()` read — the app may have changed its answer since.
    pub applied_margin: f32,
    /// True while the previous frame ran with a nonzero margin — lets the
    /// per-frame geometry publish reset state exactly once on deactivation.
    pub overflow_was_active: bool,
    /// The popover-union rect last sent via zcce set_popover_region, logical
    /// surface px; None once a clear has been sent (or never anything).
    pub sent_popover_region: Option<(i32, i32, i32, i32)>,

    pub exit: bool,
    pub redraw: bool,
    pub frame_callback_pending: bool,
    /// When the pending frame callback was armed — the starvation fallback's
    /// clock (see the render gate in `run`).
    pub frame_callback_armed_at: Option<std::time::Instant>,
    /// Keep rendering (vsync-paced) briefly after the last genuine dirty frame.
    /// Sparse, isolated commits get their frame callbacks serviced multiple
    /// compositor frames late (measured 22-128ms on cce-fx, growing per sparse
    /// commit), while a continuously committing surface is serviced in one
    /// frame (~16ms). A short warm-down keeps interactive sequences (hover,
    /// typing, scrolling) in the healthy continuous regime; idle still idles.
    pub warm_until: Option<std::time::Instant>,
    /// Consecutive renders skipped by the extent gate (pending swapchain size
    /// != the size the current logical size and scale call for). Normally 0 or
    /// 1; a persistent count means no frame is presenting and deserves a warn.
    pub extent_gate_skips: u32,
    pub first_configure_received: bool,
    pub ctrl_pressed: bool,
    /// The `undo` / `redo` chords, resolved from `input.kdl` at startup.
    pub undo_chord: String,
    /// `focus_next_group` / `focus_prev_group` (input.kdl, cce-ui domain):
    /// the plate-navigation group jump, for apps that opt in.
    pub group_next_chord: String,
    pub group_prev_chord: String,
    pub redo_chord: String,
    pub shift_pressed: bool,
    pub alt_pressed: bool,
    pub logo_pressed: bool,
    pub pressed_key: Option<PressedKey>,
    pub sender: calloop::channel::Sender<A::Message>,
    pub current_cursor_icon: Option<CursorIcon>,
    pub qh: QueueHandle<EngineState<A>>,
    pub just_configured: bool,
    pub pointer_gestures: Option<ZwpPointerGesturesV1>,
    pub pinch_gesture: Option<ZwpPointerGesturePinchV1>,
    /// The cce window-management toplevel handle, held for the window's
    /// lifetime once [`Application::utility`] declared the mode.
    pub cce_toplevel: Option<crate::protocol::cce_window_management_v1::zcce_toplevel_v1::ZcceToplevelV1>,
    /// Latest unrendered grid_patch (serial, x, y, w, h, scale) — a newer
    /// event supersedes an unconsumed older one, per protocol.
    pub pending_grid_patch: Option<(u32, f64, f64, f64, f64, f64)>,
    pub last_pinch_scale: f32,
    pub cursor_pos: (f32, f32),
    /// Serial of the most recent pointer press, kept for
    /// [`Application::take_window_action`] move/resize grabs.
    pub last_press_serial: Option<u32>,
    /// Mouse buttons currently held, as a bitmask (1 Left / 2 Right /
    /// 4 Middle). On pointer Leave mid-gesture the real Release goes to
    /// whatever surface takes the pointer next (fullscreen switches, layout
    /// animations), so Leave synthesizes releases for the held set — a drag
    /// must end, not stay armed and steered by later motion — and only then
    /// runs the off-screen hover-clear (which would otherwise corrupt the
    /// drag: a ramp key snapped to the graph corner).
    pub buttons_down: u32,
    /// This frame's display-list text, shaped and held here so the `TextSpan`s built
    /// in the render pass can borrow the buffers (Phase 6 —
    /// [`Application::display_list_text`]).
    pub dl_text_items: Vec<TextItem>,

    /// Drag-and-drop destination state (see [`crate::backend::dnd`]). The
    /// manager is absent when the compositor exposes no wl_data_device_manager;
    /// every drop path then no-ops.
    pub data_device_manager: Option<smithay_client_toolkit::data_device_manager::DataDeviceManagerState>,
    pub data_devices: Vec<smithay_client_toolkit::data_device_manager::data_device::DataDevice>,
    /// Mime type accepted for the in-flight drag; `None` means the app wants
    /// nothing this offer carries, so the drop is declined.
    pub drag_mime: Option<String>,
    /// Surface-local logical position of the last drag enter/motion — the
    /// drop point handed to [`Application::handle_drop`].
    pub drag_pos: LogicalPosition,
    /// Reader threads post completed drops here; the main loop drains it.
    pub drop_tx: Option<calloop::channel::Sender<crate::backend::dnd::DroppedData>>,
    /// The offer being read right now, held so it can be finished only once
    /// the transfer is actually done (see `dnd::drop_performed`).
    pub pending_drop_offer:
        Option<smithay_client_toolkit::data_device_manager::data_offer::DragOffer>,
    /// The input region last sent to the compositor, so a per-frame
    /// [`Application::input_regions`] only costs protocol traffic on change.
    pub applied_input_regions: Option<Vec<(i32, i32, i32, i32)>>,
}

impl<A: Application> EngineState<A> {
    pub fn init_gpu(&mut self, conn: &Connection, width_logical: f32, height_logical: f32) {
        let s = self.scale_factor as f32;
        let pw = (width_logical * s) as u32;
        let ph = (height_logical * s) as u32;

        let surface = self.surface.as_ref().expect("surface missing");

        let display_ptr = conn.backend().display_id().as_ptr() as *mut std::ffi::c_void;
        let surface_ptr = surface.id().as_ptr() as *mut std::ffi::c_void;

        let load_system_fonts = self.inner.as_ref().map_or(false, |a| a.load_system_fonts());
        // Corner radius 0: runner apps tessellate their own rounded corners.
        let renderer =
            unsafe { VkRenderer::new(display_ptr, surface_ptr, pw, ph, 0.0) };
        self.font_system = Some(if load_system_fonts {
            crate::create_font_system_with_system_fonts()
        } else {
            crate::create_font_system()
        });
        self.renderer = Some(renderer);
        self.logical_width = width_logical;
        self.logical_height = height_logical;
    }

    /// Buffer scale and physical extent for a logical size under the current
    /// scale factor: rounded, then snapped up so the extent divides by the
    /// buffer scale (a wl_surface requirement). In forced-scale mode the
    /// surface stays at buffer_scale 1 (the compositor believes scale 1).
    ///
    /// This is the single source of the buffer-size formula: `resize` sizes
    /// the swapchain with it and `render` refuses to present any extent that
    /// disagrees with it — a mispaired buffer/scale commit is how the resume
    /// output bounce halved even-sized windows (buffer at the old scale's
    /// size, new scale latched; the compositor reads it as a self-resize).
    fn buffer_geometry(scale_factor: f64, w: f32, h: f32) -> (i32, u32, u32) {
        let s = if crate::scale::forced_scale().is_some() {
            1
        } else {
            (scale_factor.round() as i32).max(1)
        };
        let su = s as u32;
        let pw = ((w as f64 * scale_factor).round() as u32).max(1).div_ceil(su) * su;
        let ph = ((h as f64 * scale_factor).round() as u32).max(1).div_ceil(su) * su;
        (s, pw, ph)
    }

    pub fn resize(&mut self, w: f32, h: f32) {
        let (w, h) = self.inner.as_ref().unwrap().adjust_size(w, h);
        if w > 0.0 && h > 0.0 {
            self.logical_width = w;
            self.logical_height = h;
            let (_, pw, ph) = Self::buffer_geometry(self.scale_factor, w, h);
            if let Some(ref mut renderer) = self.renderer {
                renderer.resize(pw, ph);
            }
            let scale = self.scale_factor;
            self.inner.as_mut().unwrap().handle_resize(w, h, scale);
            self.publish_window_geometry();
        }
    }

    /// Overflow-margin mode ([`Application::overflow_margin`]): re-publish the
    /// window frame — the surface rect inset by the margin — as the xdg window
    /// geometry, and an input region of the frame PLUS any open popover rects
    /// (an overhanging menu's rows must stay clickable; empty rim still falls
    /// through). Applied on every resize and, while the rim is live, every
    /// loop (the popover rects animate). Margin back at 0 resets both — a
    /// no-op only for apps that never had a rim. (All double-buffered surface
    /// state, latched by the next commit.)
    fn publish_window_geometry(&mut self) {
        let m = self.applied_margin;
        let Some(ref window) = self.window else { return };
        if m <= 0.0 {
            if self.overflow_was_active {
                let gw = (self.logical_width as i32).max(1);
                let gh = (self.logical_height as i32).max(1);
                window.xdg_surface().set_window_geometry(0, 0, gw, gh);
                if let Some(ref surface) = self.surface {
                    surface.set_input_region(None);
                }
            }
            return;
        }
        // Right/bottom rim: the frame keeps the surface origin — no offset,
        // frame coords == surface coords.
        let gw = ((self.logical_width - m) as i32).max(1);
        let gh = ((self.logical_height - m) as i32).max(1);
        window.xdg_surface().set_window_geometry(0, 0, gw, gh);
        if let Some(ref surface) = self.surface {
            let compositor = self.compositor_state.wl_compositor();
            let wl_region = compositor.create_region(&self.qh, ());
            wl_region.add(0, 0, gw, gh);
            // Open popovers, clamped to the surface.
            if let Some(ctx) = self.inner.as_ref().unwrap().ui_context() {
                for (_id, ptr) in ctx.tree.iter_registered() {
                    unsafe {
                        let Some(w) = ptr.as_ref() else { continue };
                        if !w.visible() {
                            continue;
                        }
                        let Some((px, py, pw, ph)) = w.popover_rect() else { continue };
                        let x0 = px.max(0.0) as i32;
                        let y0 = py.max(0.0) as i32;
                        let x1 = ((px + pw).min(self.logical_width)) as i32;
                        let y1 = ((py + ph).min(self.logical_height)) as i32;
                        if x1 > x0 && y1 > y0 {
                            wl_region.add(x0, y0, x1 - x0, y1 - y0);
                        }
                    }
                }
            }
            surface.set_input_region(Some(&wl_region));
            wl_region.destroy();
        }
    }
    
    /// Report the union of the open popover rects to the compositor
    /// (zcce set_popover_region, manager v7), so its window chrome — the
    /// overview resize ring — stays out from under an in-surface menu. Sent
    /// only on change, and a clear is sent when the last popover closes;
    /// rects are clamped to the surface in logical px, the coordinate space
    /// the protocol specifies. Popovers animate, so this runs every loop —
    /// the change gate is what keeps it quiet.
    fn send_popover_region(&mut self) {
        let Some(tl) = &self.cce_toplevel else { return };
        // Version gate on the MANAGER numbering the resource carries (the
        // toplevel inherits its bind version): 7 is where the request
        // appeared. An older compositor would kill the client on the
        // unknown opcode.
        if tl.version() < 7 {
            return;
        }
        let mut union: Option<(f32, f32, f32, f32)> = None;
        if let Some(ctx) = self.inner.as_ref().unwrap().ui_context() {
            for (_id, ptr) in ctx.tree.iter_registered() {
                unsafe {
                    let Some(w) = ptr.as_ref() else { continue };
                    if !w.visible() {
                        continue;
                    }
                    let Some((px, py, pw, ph)) = w.popover_rect() else { continue };
                    let (x0, y0) = (px.max(0.0), py.max(0.0));
                    let x1 = (px + pw).min(self.logical_width);
                    let y1 = (py + ph).min(self.logical_height);
                    if x1 <= x0 || y1 <= y0 {
                        continue;
                    }
                    union = Some(match union {
                        None => (x0, y0, x1, y1),
                        Some((ux0, uy0, ux1, uy1)) => {
                            (ux0.min(x0), uy0.min(y0), ux1.max(x1), uy1.max(y1))
                        }
                    });
                }
            }
        }
        let next = union.map(|(x0, y0, x1, y1)| {
            (x0 as i32, y0 as i32, (x1 - x0).ceil() as i32, (y1 - y0).ceil() as i32)
        });
        if next == self.sent_popover_region {
            return;
        }
        match next {
            Some((x, y, w, h)) => tl.set_popover_region(x, y, w, h),
            None => tl.set_popover_region(0, 0, 0, 0),
        }
        self.sent_popover_region = next;
    }

    /// The cursor for the pointer at (lx, ly): the app's
    /// [`Application::cursor_icon`] override, else the standard-CSD edge
    /// cursors (status bars and non-standard-CSD apps fall back to Default).
    fn cursor_icon_at(&self, lx: f32, ly: f32) -> CursorIcon {
        let inner = self.inner.as_ref().unwrap();
        if let Some(icon) = inner.cursor_icon(lx, ly) {
            return icon;
        }
        if inner.settings().app_id.starts_with("cce-status")
            || !inner.standard_csd()
            || !inner.csd_resize_borders()
        {
            return CursorIcon::Default;
        }
        let border = 8.0f32;
        if ly < border {
            if lx < border {
                CursorIcon::NwResize
            } else if lx > self.logical_width - border {
                CursorIcon::NeResize
            } else {
                CursorIcon::NResize
            }
        } else if ly > self.logical_height - border {
            if lx < border {
                CursorIcon::SwResize
            } else if lx > self.logical_width - border {
                CursorIcon::SeResize
            } else {
                CursorIcon::SResize
            }
        } else if lx < border {
            CursorIcon::WResize
        } else if lx > self.logical_width - border {
            CursorIcon::EResize
        } else {
            CursorIcon::Default
        }
    }

    pub fn render(&mut self) {
        // Grid patch: resize to the patch's buffer size, tell the app what
        // world region this frame covers, and ack — the commit this render
        // produces is the one the compositor latches at the new anchor.
        if let Some((serial, px, py, pw, ph, pscale)) = self.pending_grid_patch.take() {
            self.resize((pw * pscale) as f32, (ph * pscale) as f32);
            self.inner.as_mut().unwrap().grid_patch(px, py, pw, ph, pscale);
            if let Some(tl) = &self.cce_toplevel {
                tl.ack_grid_patch(serial);
            }
        }
        let logical_w = self.logical_width;
        let logical_h = self.logical_height;
        let scale_factor = self.scale_factor;

        if let Some(ref surface) = self.surface {
            if let Some(regions) = self.inner.as_ref().unwrap().input_regions() {
                // Only re-send when it actually changes. This runs per frame,
                // and a client whose region tracks its content (the desktop
                // grid's items follow every pan) would otherwise create and
                // destroy a wl_region on every frame of a camera flight.
                if self.applied_input_regions.as_deref() != Some(regions.as_slice()) {
                    let compositor = self.compositor_state.wl_compositor();
                    let wl_region = compositor.create_region(&self.qh, ());
                    for &(rx, ry, rw, rh) in &regions {
                        wl_region.add(rx, ry, rw, rh);
                    }
                    surface.set_input_region(Some(&wl_region));
                    wl_region.destroy();
                    self.applied_input_regions = Some(regions);
                }
            }
        }
        
        // 0. Shape every registered widget against the SAME FontSystem the glyph pass draws
        // with, before the app builds its frame. A widget's caret/selection/click→index math
        // reads per-glyph advances its `prepare_text` records; nothing else calls it on the
        // display-list path (the paint walk is `&dyn`, and apps were left to remember —
        // cce-list, cce-secrets, and the reference DemoApp all forgot, so their carets fell
        // back to `measure_text_width("M")`, an inked extent that drifts off the glyphs).
        // The flat path shapes in `layout::render_widget`; apps that hand-shape still work —
        // their call and this one hit the same shaped-buffer cache. Pointers are collected
        // first so the registry borrow ends before any widget is mutated (the missed-press
        // walk dereferences the same registry the same way).
        {
            let ptrs: Vec<*mut (dyn crate::widget::WidgetHost + 'static)> = self
                .inner
                .as_ref()
                .unwrap()
                .ui_context()
                .map(|ctx| ctx.tree.iter_registered().map(|(_, p)| p).collect())
                .unwrap_or_default();
            if !ptrs.is_empty() {
                let fs = self.font_system.as_mut().unwrap();
                for ptr in ptrs {
                    unsafe {
                        if let Some(w) = ptr.as_mut() {
                            w.prepare_text(fs);
                        }
                    }
                }
            }
        }

        // 1. The frame's geometry IS the app's display list — the single paint path. Tessellated
        // below as one batched, GPU-scissor-clipped pass. An app that draws nothing returns
        // `None`, giving an empty frame (the legacy view*/tuple-wrapping path is gone).
        let dl = self.inner.as_mut().unwrap()
            .display_list(LogicalSize::new(logical_w, logical_h), scale_factor)
            .unwrap_or_else(|| crate::scene::paint::PaintCtx::new().finish());

        // 1a. Phase 6 display-list text: shape the list's Text prims through the shared buffer
        // cache and hold them for the glyph pass (the TextSpans built below borrow these).
        // Clip = the paint walk's item clip ∩ the prim's own bounds, in logical space.
        self.dl_text_items.clear();
        if self.inner.as_ref().unwrap().display_list_text() {
            let fs = self.font_system.as_mut().unwrap();
            for item in &dl.items {
                if let crate::scene::paint::Prim::Text { text, x, y, font_size, color, alpha, font, bounds, attrs, layout } = &item.prim {
                    let clip = item.clip.map(|c| [c.x, c.y, c.x + c.width, c.y + c.height]);
                    let merged = match (clip, *bounds) {
                        (Some(a), Some(b)) => Some([a[0].max(b[0]), a[1].max(b[1]), a[2].min(b[2]), a[3].min(b[3])]),
                        (Some(a), None) => Some(a),
                        (None, b) => b,
                    };
                    // Boxed text (wrap/align) shapes uncached and shifts down by the vertical
                    // offset; ordinary labels take the shared cached buffer.
                    let (buffer, y_off) = match layout {
                        Some(l) => get_text_buffer_laid_out(fs, text, *font_size, font.as_deref(), *attrs, *l),
                        None => (get_text_buffer_attrs(fs, text, *font_size, font.as_deref(), *attrs), 0.0),
                    };
                    self.dl_text_items.push(TextItem {
                        buffer,
                        x: *x,
                        y: *y + y_off,
                        color: cosmic_text::Color::rgba(
                            color[0],
                            color[1],
                            color[2],
                            (alpha.clamp(0.0, 1.0) * 255.0).round() as u8,
                        ),
                        bounds: merged,
                        clip_circle: item.clip_circle,
                        clip_rrect: item.clip_rrect,
                    });
                }
            }
        }

        let (mut verts, mut dl_batches, dl_images, plate_features) = tessellate_display_list(&dl, logical_w, logical_h, scale_factor as f32);
        // A pending height-field export (`CCE_HEIGHTMAP`, or an app's
        // `scene::heightfield::request`): the plates of THIS frame, sampled
        // as the geometry the shader is about to shade.
        if let Some(req) = crate::scene::heightfield::take_request() {
            let s = scale_factor as f32;
            let (pw, ph) = ((logical_w * s).round() as usize, (logical_h * s).round() as usize);
            let hf = crate::scene::heightfield::HeightField::from_frame(&dl_batches, &plate_features, pw, ph, s);
            let (lo, hi) = hf.range_px();
            match crate::scene::heightfield::export_png(&hf, &req.path, req.mm_per_sample) {
                Ok(()) => log::info!(
                    "[heightfield] wrote {} ({}x{} px, {:.3}..{:.3} mm, metric {})",
                    req.path.display(), pw, ph, lo / hf.px_per_mm, hi / hf.px_per_mm, hf.source.as_str()
                ),
                Err(e) => log::warn!("[heightfield] export to {} failed: {e}", req.path.display()),
            }
        }
        // custom_vertices (e.g. graph geometry) is appended as a final unclipped batch drawn on top.
        let pre_custom = verts.len() as u32;
        self.inner.as_mut().unwrap().custom_vertices(&mut verts, LogicalSize::new(logical_w, logical_h), scale_factor);
        if (verts.len() as u32) > pre_custom {
            dl_batches.push(DlBatch { scissor: None, clip_rrect: None, start: pre_custom, end: verts.len() as u32, plate: None, blur_behind: false });
        }

        // 1b. Overlay quads (drawn after the text pass).
        let mut overlay_quads = Vec::new();
        self.inner.as_mut().unwrap().overlay_quads(&mut overlay_quads, LogicalSize::new(logical_w, logical_h), scale_factor);
        let mut overlay_verts = Vec::new();
        for &(qx, qy, qw, qh, qc) in &overlay_quads {
            overlay_verts.extend(quad_vertices(qx, qy, qw, qh, logical_w, logical_h, qc));
        }

        // 2. Prepare text
        let scale_f32 = scale_factor as f32;
        let pw = (logical_w * scale_f32) as u32;
        let ph = (logical_h * scale_f32) as u32;

        let bounds = TextBounds { left: 0, top: 0, right: pw as i32, bottom: ph as i32 };
        // All text is display-list text now (the legacy text_items/text_areas path is gone):
        // map each dl Text prim with the default mapping (scale + surface clamp) plus the
        // popover-occlusion clamp against the app's registered popovers.
        let mut dl_overlay_rects: Vec<(f32, f32, f32, f32)> = Vec::new();
        if let Some(ctx) = self.inner.as_ref().unwrap().ui_context() {
            for &pop_id in &ctx.active_popovers {
                if let Some(ptr) = ctx.tree.get_ptr(pop_id) {
                    unsafe {
                        if let Some((x, y, w, h)) = (*ptr).popover_rect() {
                            dl_overlay_rects.push((x, y, w, h));
                        }
                    }
                }
            }
        }
        // The global context menu draws into the app's display list (the render-only xdg
        // popup is gone), so it gets the same occlusion: the menu rect clamps list text
        // beneath, and the menu's own labels are exempt because they carry bounds equal
        // to the rect.
        if crate::widget::context_menu::is_visible() {
            dl_overlay_rects.push((
                crate::widget::context_menu::x(),
                crate::widget::context_menu::y(),
                crate::widget::context_menu::w(),
                crate::widget::context_menu::h(),
            ));
        }
        let mut spans: Vec<TextSpan> = Vec::new();
        for ti in &self.dl_text_items {
            let mut item_bounds = if let Some([l, t, r, b]) = ti.bounds {
                TextBounds {
                    left: ((l * scale_f32).round() as i32).clamp(0, bounds.right),
                    top: ((t * scale_f32).round() as i32).clamp(0, bounds.bottom),
                    right: ((r * scale_f32).round() as i32).clamp(0, bounds.right),
                    bottom: ((b * scale_f32).round() as i32).clamp(0, bounds.bottom),
                }
            } else {
                bounds
            };
            popover_occlusion_clamp(&dl_overlay_rects, ti, scale_f32, &mut item_bounds);
            spans.push(TextSpan {
                buffer: &ti.buffer,
                left: (ti.x * scale_f32).round(),
                top: (ti.y * scale_f32).round(),
                // Buffers are shaped at physical size (get_text_buffer_attrs).
                scale: 1.0,
                bounds: Some([
                    item_bounds.left,
                    item_bounds.top,
                    item_bounds.right,
                    item_bounds.bottom,
                ]),
                default_color: [
                    ti.color.r() as f32 / 255.0,
                    ti.color.g() as f32 / 255.0,
                    ti.color.b() as f32 / 255.0,
                    ti.color.a() as f32 / 255.0,
                ],
                rotation: None,
                // Circle wins when both are set (the circular pane's innermost clip);
                // otherwise a rounded-rect clip rides as center+radius with extents.
                clip_circle: match (ti.clip_circle, ti.clip_rrect) {
                    (Some(c), _) => [c[0] * scale_f32, c[1] * scale_f32, c[2] * scale_f32],
                    (None, Some(rr)) => [rr[0] * scale_f32, rr[1] * scale_f32, rr[4] * scale_f32],
                    (None, None) => [0.0; 3],
                },
                clip_extents: match (ti.clip_circle, ti.clip_rrect) {
                    (None, Some(rr)) => [rr[2] * scale_f32, rr[3] * scale_f32],
                    _ => [0.0; 2],
                },
            });
        }

        // 3. Frame: display-list batches under their physical scissors, then
        // text, then overlays. The renderer owns swapchain rebuild/recovery.
        // An app without display-list text owns the renderer's text state
        // itself (it stages via stage_renderer below); don't wipe it here.
        let renderer = self.renderer.as_mut().unwrap();
        if self.inner.as_ref().unwrap().display_list_text() {
            renderer.prepare_text(self.font_system.as_mut().unwrap(), &mut self.swash_cache, &spans);
        }

        let image_quads: Vec<crate::vk::ImageQuad> = dl_images
            .iter()
            .map(|di| crate::vk::ImageQuad {
                image: di.image,
                rect: (
                    di.rect.x * scale_f32,
                    di.rect.y * scale_f32,
                    di.rect.width * scale_f32,
                    di.rect.height * scale_f32,
                ),
                alpha: di.alpha,
                z_before: di.at,
                clip: di.clip.map(|c| {
                    (
                        (c.x * scale_f32).max(0.0) as u32,
                        (c.y * scale_f32).max(0.0) as u32,
                        (c.width * scale_f32) as u32,
                        (c.height * scale_f32) as u32,
                    )
                }),
            })
            .collect();

        let batches: Vec<Batch2D> = dl_batches
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
            .collect();

        let cc = self.inner.as_ref().unwrap().clear_color();
        let clear_color = [cc[0].powf(2.2), cc[1].powf(2.2), cc[2].powf(2.2), cc[3]];

        // Commit the buffer scale together with a buffer it is legal for: the
        // present inside draw_frame_2d is the only commit on this surface, so
        // sending the request here orders it right before a matching-size
        // attach+commit.
        //
        // Present only the EXACT extent the current logical size and scale
        // call for. Divisibility is not enough: mid scale-transition (the
        // resume output bounce) the pending extent can belong to the other
        // scale, and an even-sized old-scale buffer divides cleanly by the
        // new scale — the commit is protocol-legal, so the compositor reads
        // it as a self-resize to half/double and reconfigures the window to
        // match (how the color editor came back from suspend at exactly half
        // size with the divisibility guard green). Odd sizes at least die
        // loudly (invalid_size). On mismatch, re-request the right extent
        // and skip — before the frame-callback request below, so the loop
        // isn't left waiting on a callback no commit will ever latch.
        if let Some(ref surface) = self.surface {
            let (s, epw, eph) =
                Self::buffer_geometry(self.scale_factor, self.logical_width, self.logical_height);
            let e = renderer.pending_extent();
            if e.width != epw || e.height != eph {
                renderer.resize(epw, eph);
                self.extent_gate_skips += 1;
                // ~5s of continuous skipping at the 16ms loop cadence: nothing
                // is presenting and nothing else will say so — this is the
                // only witness to a wedged pending extent.
                if self.extent_gate_skips % 300 == 0 {
                    log::warn!(
                        "[window_runner] extent gate: pending {}x{} != expected {}x{} for {} consecutive renders; no frame is presenting",
                        e.width, e.height, epw, eph, self.extent_gate_skips,
                    );
                }
                self.redraw = true;
                return;
            }
            self.extent_gate_skips = 0;
            if s != self.committed_buffer_scale {
                surface.set_buffer_scale(s);
                self.committed_buffer_scale = s;
            }
        }

        if let Some(ref surface) = self.surface {
            let _callback = surface.frame(&self.qh, ());
            self.frame_callback_pending = true;
            self.frame_callback_armed_at = Some(std::time::Instant::now());
            if std::env::var("CCE_PRESENT_DEBUG").is_ok() {
                let t = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() % 100000;
                eprintln!("[vk] t={} armed frame callback", t);
            }
        }

        // Direct renderer staging (3D scenes, RT panes, app-shaped text).
        if self.inner.as_mut().unwrap().stage_renderer(
            renderer,
            LogicalSize::new(logical_w, logical_h),
            scale_factor,
        ) {
            self.redraw = true;
        }

        if !renderer.draw_frame_2d(Frame2D {
            verts: &verts,
            batches: &batches,
            overlay_verts: &overlay_verts,
            images: &image_quads,
            plate_features: &plate_features,
            clear_color,
        }) {
            // No present happened (swapchain out-of-date, or the created
            // swapchain didn't match the requested extent). The frame
            // callback requested above will never latch without a commit —
            // clear it or the demand-driven loop stalls waiting forever.
            self.frame_callback_pending = false;
            self.redraw = true;
        }
    }
}

impl<A: Application> Drop for EngineState<A> {
    fn drop(&mut self) {
        self.renderer = None;
    }
}

impl<A: Application> CompositorHandler for EngineState<A> {
    fn scale_factor_changed(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        scale_factor: i32,
    ) {
        // Don't send set_buffer_scale here: an in-flight present can commit an
        // old-scale-sized buffer right after it, which is a fatal invalid_size
        // protocol error (seen on resume, when outputs bounce 2→1→2). The scale
        // request is sent in `render`, paired with a matching-size present.
        if crate::scale::forced_scale().is_some() {
            // Forced mode: the compositor's opinion (scale 1 under cage) must
            // not clobber the override.
            return;
        }
        if self.inner.as_ref().map_or(false, |a| a.grid()) {
            // Grid surfaces stay at scale 1 — patch.scale is the sole
            // resolution authority (see the pin at surface creation).
            return;
        }
        // Resume bounce: when the surface sits on no LIVE output (the DRM
        // connector was destroyed and not yet re-created), the reported
        // factor is SCTK's no-outputs fallback, not information — hold the
        // last real scale. When the reborn output arrives, surface enter
        // recomputes and this handler runs again with a live output backing
        // it. Liveness matters (not just enter/leave counting): the leave
        // for a destroyed output may never be delivered.
        let on_live_output = self
            .entered_outputs
            .iter()
            .any(|o| self.output_state.info(o).is_some());
        if !on_live_output && (scale_factor as f64) < self.scale_factor {
            return;
        }
        self.scale_factor = scale_factor as f64;
        self.resize(self.logical_width, self.logical_height);
        self.redraw = true;
    }
    
    fn transform_changed(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        _new_transform: wl_output::Transform,
    ) {}
    
    fn frame(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        _time: u32,
    ) {}
    
    fn surface_enter(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        output: &wl_output::WlOutput,
    ) {
        if !self.entered_outputs.contains(output) {
            self.entered_outputs.push(output.clone());
        }
        // Dead entries (destroyed outputs never send leave) are harmless —
        // the liveness check in scale_factor_changed skips them — but drop
        // them here so the list doesn't grow across suspend cycles.
        self.entered_outputs
            .retain(|o| self.output_state.info(o).is_some());
        self.redraw = true;
    }

    fn surface_leave(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        output: &wl_output::WlOutput,
    ) {
        self.entered_outputs.retain(|o| o != output);
    }
}

impl<A: Application> OutputHandler for EngineState<A> {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.output_state
    }
    
    fn new_output(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, _output: wl_output::WlOutput) {
        let scale = crate::wayland::detect_scale_factor(&self.output_state);
        crate::scale::set_scale_factor(scale as f32);
        crate::units::set_metric(crate::wayland::detect_metric(&self.output_state, scale));
    }
    fn update_output(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, _output: wl_output::WlOutput) {
        let scale = crate::wayland::detect_scale_factor(&self.output_state);
        crate::scale::set_scale_factor(scale as f32);
        crate::units::set_metric(crate::wayland::detect_metric(&self.output_state, scale));
    }
    fn output_destroyed(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, _output: wl_output::WlOutput) {}
}

impl<A: Application> ShmHandler for EngineState<A> {
    fn shm_state(&mut self) -> &mut Shm {
        &mut self.shm_state
    }
}

impl<A: Application> ProvidesRegistryState for EngineState<A> {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry_state
    }
    
    fn runtime_add_global(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _name: u32,
        _interface: &str,
        _version: u32,
    ) {}
    
    fn runtime_remove_global(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _name: u32,
        _interface: &str,
    ) {}
}

impl<A: Application> WindowHandler for EngineState<A> {
    fn configure(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _window: &XdgWindow,
        configure: WindowConfigure,
        _serial: u32,
    ) {
        let is_fs = configure.is_fullscreen();
        let is_max = configure.is_maximized();
        crate::scale::set_fullscreen(is_fs);
        crate::scale::set_maximized(is_max);

        let (w, h) = configure.new_size;
        // Configure sizes are window-geometry sizes; with an overflow margin
        // the surface is a rim larger on the right and bottom.
        let m = self.inner.as_ref().unwrap().overflow_margin() as f32;
        if let (Some(w), Some(h)) = (w, h) {
            let width = w.get();
            let height = h.get();
            // Forced mode: the compositor's logical size is really physical
            // pixels (scale-1 output); divide to get the app's logical space.
            let f = crate::scale::forced_scale().unwrap_or(1.0);
            self.frame_logical = (width as f32 / f, height as f32 / f);
            self.applied_margin = m;
            self.resize(width as f32 / f + m, height as f32 / f + m);
        } else if self.inner.as_ref().unwrap().grid() && self.logical_width > 1.0 {
            // A grid app's size belongs to its PATCHES: the compositor's
            // "you choose" 0x0 must not bounce the surface back to the
            // settings size — that thrash recreated multi-hundred-MB
            // swapchains per bounce (6.3G peak in 10s). Keep the current
            // size; the next grid_patch is the only resizer.
        } else {
            let settings = self.inner.as_ref().unwrap().settings();
            self.frame_logical = (settings.width as f32, settings.height as f32);
            self.applied_margin = m;
            self.resize(settings.width as f32 + m, settings.height as f32 + m);
        }
        self.redraw = true;
        self.frame_callback_pending = false;
        self.first_configure_received = true;
        self.just_configured = true;
    }

    fn request_close(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, _window: &XdgWindow) {
        self.exit = true;
    }
}

impl<A: Application> LayerShellHandler for EngineState<A> {
    fn closed(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, _layer: &LayerSurface) {
        self.exit = true;
    }

    fn configure(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _layer: &LayerSurface,
        configure: LayerSurfaceConfigure,
        _serial: u32,
    ) {
        // new_size is in logical pixels; 0 means "client decides", so fall back
        // to the app's requested size (mirrors the xdg WindowHandler above).
        let (w, h) = configure.new_size;
        if w > 0 && h > 0 {
            self.resize(w as f32, h as f32);
        } else {
            let settings = self.inner.as_ref().unwrap().settings();
            self.resize(settings.width as f32, settings.height as f32);
        }
        self.redraw = true;
        self.frame_callback_pending = false;
        self.first_configure_received = true;
        self.just_configured = true;
    }
}

impl<A: Application> SeatHandler for EngineState<A> {
    fn seat_state(&mut self) -> &mut SeatState {
        &mut self.seat_state
    }
    
    fn new_seat(&mut self, _conn: &Connection, qh: &QueueHandle<Self>, seat: wl_seat::WlSeat) {
        self.ensure_data_device(qh, &seat);
        self.seats.push(seat);
    }
    
    fn new_capability(
        &mut self,
        _conn: &Connection,
        qh: &QueueHandle<Self>,
        seat: wl_seat::WlSeat,
        capability: Capability,
    ) {
        // Every seat arrives here, unlike `new_seat` — SCTK binds the seats
        // that already exist at startup without announcing them, so a device
        // created only there is never created at all on a normal launch.
        self.ensure_data_device(qh, &seat);
        if capability == Capability::Pointer && self.pointer.is_none() {
            let surface = self.compositor_state.create_surface::<Self>(qh);
            let themed_pointer = self.seat_state.get_pointer_with_theme(
                qh,
                &seat,
                self.shm_state.wl_shm(),
                surface,
                ThemeSpec::System,
            ).unwrap();
            if let Some(ref pg) = self.pointer_gestures {
                self.pinch_gesture = Some(pg.get_pinch_gesture(themed_pointer.pointer(), qh, ()));
            }
            self.pointer = Some(themed_pointer);
        }
        if capability == Capability::Keyboard && self.keyboard.is_none() {
            let keyboard = self.seat_state.get_keyboard(qh, &seat, None).unwrap();
            self.keyboard = Some(keyboard);
        }
    }
    
    fn remove_capability(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _seat: wl_seat::WlSeat,
        capability: Capability,
    ) {
        if capability == Capability::Pointer {
            self.pinch_gesture = None;
            self.pointer = None;
        }
        if capability == Capability::Keyboard {
            self.keyboard = None;
        }
    }
    
    fn remove_seat(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, seat: wl_seat::WlSeat) {
        self.seats.retain(|s| s != &seat);
    }
}

impl<A: Application> PointerHandler for EngineState<A> {
    fn pointer_frame(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _pointer: &wl_pointer::WlPointer,
        events: &[smithay_client_toolkit::seat::pointer::PointerEvent],
    ) {
        use smithay_client_toolkit::seat::pointer::PointerEventKind;
        let mut coalesced_h = 0.0f64;
        let mut coalesced_v = 0.0f64;
        let mut discrete_h = 0;
        let mut discrete_v = 0;
        let mut has_scroll = false;
        let mut axis_source: Option<wl_pointer::AxisSource> = None;
        let mut axis_stop = false;
        let (mut last_lx, mut last_ly) = (0.0f32, 0.0f32);

        // Forced mode: pointer positions arrive in the compositor's scale-1
        // logical space (= physical); divide into the app's logical space.
        let forced = crate::scale::forced_scale().unwrap_or(1.0);
        for event in events {
            let (x, y) = event.position;
            // Overflow-margin mode needs no translation: the rim is
            // right/bottom-only, so frame coords == surface coords.
            let lx = x as f32 / forced;
            let ly = y as f32 / forced;

            self.cursor_pos = (lx, ly);
            match &event.kind {
                PointerEventKind::Enter { .. } => {
                    // Enter carries the pointer's position but no Motion follows until it
                    // actually moves — without this the app's hover state is stale from
                    // enter to first move, and a press in that window can misroute (e.g. a
                    // divider press falling through to the movable-root plate window drag).
                    let mut rebuild = false;
                    self.inner.as_mut().unwrap().handle_pointer_move(LogicalPosition::new(lx, ly), &mut rebuild);
                    if rebuild {
                        self.redraw = true;
                    }

                    let cursor_icon = self.cursor_icon_at(lx, ly);
                    self.current_cursor_icon = Some(cursor_icon);
                    if let Some(ref themed_pointer) = self.pointer {
                        let _ = themed_pointer.set_cursor(_conn, cursor_icon);
                    }
                }
                PointerEventKind::Leave { .. } => {
                    self.current_cursor_icon = None;
                    // Focus can move mid-gesture (a fullscreen switch, a
                    // relayout sliding the window away): the real Release
                    // then lands on another surface, and an armed drag would
                    // live forever, steered by whatever motion arrives next.
                    // End held gestures with synthetic releases at the last
                    // known cursor position before anything else.
                    if self.buttons_down != 0 {
                        let (px, py) = self.cursor_pos;
                        for (bit, btn) in
                            [(1u32, MouseButton::Left), (2, MouseButton::Right), (4, MouseButton::Middle)]
                        {
                            if self.buttons_down & bit == 0 {
                                continue;
                            }
                            let mut rebuild = false;
                            if let Some(msg) = self.inner.as_mut().unwrap().handle_mouse_input(
                                btn,
                                ElementState::Released,
                                LogicalPosition::new(px, py),
                                &mut rebuild,
                            ) {
                                let mut update_rebuild = false;
                                self.inner.as_mut().unwrap().update(msg, &mut update_rebuild, &mut self.exit);
                                if update_rebuild {
                                    rebuild = true;
                                }
                            }
                            if rebuild {
                                self.redraw = true;
                            }
                        }
                        self.buttons_down = 0;
                    }
                    // Then clear hover with an off-screen move — safe now
                    // that no drag is held.
                    let mut rebuild = false;
                    self.inner.as_mut().unwrap().handle_pointer_move(LogicalPosition::new(-10000.0, -10000.0), &mut rebuild);
                    if rebuild {
                        self.redraw = true;
                    }
                }
                PointerEventKind::Motion { .. } => {
                    let mut rebuild = false;
                    self.inner.as_mut().unwrap().handle_pointer_move(LogicalPosition::new(lx, ly), &mut rebuild);
                    if rebuild {
                        self.redraw = true;
                    }

                    let cursor_icon = self.cursor_icon_at(lx, ly);

                    if self.current_cursor_icon != Some(cursor_icon) {
                        self.current_cursor_icon = Some(cursor_icon);
                        if let Some(ref themed_pointer) = self.pointer {
                            let _ = themed_pointer.set_cursor(_conn, cursor_icon);
                        }
                    }
                }
                PointerEventKind::Press { button, serial, .. } => {
                    let btn = match *button {
                        272 => MouseButton::Left,
                        273 => MouseButton::Right,
                        274 => MouseButton::Middle,
                        _ => continue,
                    };
                    self.last_press_serial = Some(*serial);
                    self.buttons_down |= match btn {
                        MouseButton::Left => 1,
                        MouseButton::Right => 2,
                        _ => 4,
                    };

                    // Client-Side Decorations (CSD) Drag & Resize Handling
                    let is_status_bar = self.inner.as_ref().unwrap().settings().app_id.starts_with("cce-status");
                    if btn == MouseButton::Left && !is_status_bar && self.inner.as_ref().unwrap().standard_csd() {
                        let border = 8.0f32;
                        let mut edge = smithay_client_toolkit::reexports::protocols::xdg::shell::client::xdg_toplevel::ResizeEdge::None;
                        if !self.inner.as_ref().unwrap().csd_resize_borders() {
                            // Resize borders are off: the compositor's own band
                            // outside the window handles it. Fall through to the
                            // move checks so drag-to-move still works.
                        } else if ly < border {
                            if lx < border {
                                edge = smithay_client_toolkit::reexports::protocols::xdg::shell::client::xdg_toplevel::ResizeEdge::TopLeft;
                            } else if lx > self.logical_width - border {
                                edge = smithay_client_toolkit::reexports::protocols::xdg::shell::client::xdg_toplevel::ResizeEdge::TopRight;
                            } else {
                                edge = smithay_client_toolkit::reexports::protocols::xdg::shell::client::xdg_toplevel::ResizeEdge::Top;
                            }
                        } else if ly > self.logical_height - border {
                            if lx < border {
                                edge = smithay_client_toolkit::reexports::protocols::xdg::shell::client::xdg_toplevel::ResizeEdge::BottomLeft;
                            } else if lx > self.logical_width - border {
                                edge = smithay_client_toolkit::reexports::protocols::xdg::shell::client::xdg_toplevel::ResizeEdge::BottomRight;
                            } else {
                                edge = smithay_client_toolkit::reexports::protocols::xdg::shell::client::xdg_toplevel::ResizeEdge::Bottom;
                            }
                        } else if lx < border {
                            edge = smithay_client_toolkit::reexports::protocols::xdg::shell::client::xdg_toplevel::ResizeEdge::Left;
                        } else if lx > self.logical_width - border {
                            edge = smithay_client_toolkit::reexports::protocols::xdg::shell::client::xdg_toplevel::ResizeEdge::Right;
                        }

                        if edge != smithay_client_toolkit::reexports::protocols::xdg::shell::client::xdg_toplevel::ResizeEdge::None {
                            if let Some(ref window) = self.window {
                                let seat_owned = self.seats.first().cloned().or_else(|| self.seat_state.seats().next());
                                if let Some(ref seat) = seat_owned {
                                    window.resize(seat, *serial, edge);
                                    continue;
                                }
                            }
                        }

                        // Titlebar drag check: y is in [8.0, 32.0], and x is not in the top-right button area
                        let mut should_move = false;
                        let mut is_widget = false;
                        if let Some(ctx) = self.inner.as_ref().unwrap().ui_context() {
                            if ctx.is_widget_at(lx, ly) {
                                is_widget = true;
                            }
                        }
                        if !is_widget
                            && self.inner.as_ref().unwrap().csd_titlebar_move()
                            && ly >= border && ly < 32.0 && lx < self.logical_width - 70.0
                        {
                            should_move = true;
                        } else if self.inner.as_ref().unwrap().is_movable_root_plate_at(lx, ly) {
                            should_move = true;
                        }

                        if should_move {
                            if let Some(ref window) = self.window {
                                let seat_owned = self.seats.first().cloned().or_else(|| self.seat_state.seats().next());
                                if let Some(ref seat) = seat_owned {
                                    window.move_(seat, *serial);
                                    continue;
                                }
                            }
                        }
                    }

                    // Outside-press close for open popovers, BEFORE the app's
                    // dispatch: apps commonly region-gate their routing, so an
                    // open menu's owner may never hear about a press elsewhere.
                    if btn == MouseButton::Left {
                        if let Some(ctx) = self.inner.as_mut().unwrap().ui_context_mut() {
                            ctx.close_popovers_missed_by_press(lx, ly);
                        }
                    }

                    let mut rebuild = false;
                    if let Some(msg) = self.inner.as_mut().unwrap().handle_mouse_input(btn, ElementState::Pressed, LogicalPosition::new(lx, ly), &mut rebuild) {
                        let mut update_rebuild = false;
                        self.inner.as_mut().unwrap().update(msg, &mut update_rebuild, &mut self.exit);
                        if update_rebuild {
                            rebuild = true;
                        }
                    }
                    if rebuild {
                        self.redraw = true;
                    }
                }
                PointerEventKind::Release { button, .. } => {
                    let btn = match *button {
                        272 => MouseButton::Left,
                        273 => MouseButton::Right,
                        274 => MouseButton::Middle,
                        _ => continue,
                    };
                    self.buttons_down &= !match btn {
                        MouseButton::Left => 1,
                        MouseButton::Right => 2,
                        _ => 4,
                    };
                    let mut rebuild = false;
                    if let Some(msg) = self.inner.as_mut().unwrap().handle_mouse_input(btn, ElementState::Released, LogicalPosition::new(lx, ly), &mut rebuild) {
                        let mut update_rebuild = false;
                        self.inner.as_mut().unwrap().update(msg, &mut update_rebuild, &mut self.exit);
                        if update_rebuild {
                            rebuild = true;
                        }
                    }
                    if rebuild {
                        self.redraw = true;
                    }
                }
                PointerEventKind::Axis { horizontal, vertical, source, .. } => {
                    coalesced_h += horizontal.absolute;
                    coalesced_v += vertical.absolute;
                    discrete_h += horizontal.discrete;
                    discrete_v += vertical.discrete;
                    // The source and the finger-lift stop ride in the same
                    // frame as the deltas (or alone, for the lift): they
                    // decide the smooth-scroll phase below.
                    if source.is_some() {
                        axis_source = *source;
                    }
                    axis_stop |= horizontal.stop || vertical.stop;
                    last_lx = lx;
                    last_ly = ly;
                    has_scroll = true;
                }
            }
        }

        if has_scroll {
            // Per-app scroll factors from input.kdl (`<app>`/`cce-ui` domain
            // `input { }` blocks); the compositor's global device scaling has
            // already been applied at the source.
            let factors = crate::input::scroll_factors();
            // Smooth-scroll phase for this dispatch: a finger lift is a stop
            // frame (no delta); finger/continuous sources track 1:1 and may
            // fling on the lift; everything else is a wheel notch that glides.
            let no_delta = coalesced_h == 0.0 && coalesced_v == 0.0 && discrete_h == 0 && discrete_v == 0;
            let phase = if axis_stop && no_delta {
                crate::widget::ScrollPhase::FingerEnd
            } else if discrete_h == 0 && discrete_v == 0
                && matches!(
                    axis_source,
                    None | Some(wl_pointer::AxisSource::Finger) | Some(wl_pointer::AxisSource::Continuous)
                )
            {
                crate::widget::ScrollPhase::Finger
            } else {
                crate::widget::ScrollPhase::Wheel
            };
            crate::widget::scroll_motion::set_scroll_phase(phase);
            let delta = if discrete_h == 0 && discrete_v == 0 {
                // Pixel scroll event from touchpad / smooth mouse
                MouseScrollDelta::PixelDelta(Position {
                    x: -coalesced_h * factors.trackpad,
                    y: -coalesced_v * factors.trackpad,
                })
            } else {
                // Discrete scroll event (e.g. wheel clicks)
                let h_lines = if discrete_h != 0 { discrete_h as f32 } else { coalesced_h as f32 / 10.0 };
                let v_lines = if discrete_v != 0 { discrete_v as f32 } else { coalesced_v as f32 / 10.0 };
                MouseScrollDelta::LineDelta(-h_lines * factors.mouse as f32, -v_lines * factors.mouse as f32)
            };
            if crate::scroll_debug() {
                static T0: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();
                let t = T0.get_or_init(std::time::Instant::now).elapsed().as_millis();
                eprintln!(
                    "[scroll {t}ms] runner: coalesced=({coalesced_h:.2},{coalesced_v:.2}) discrete=({discrete_h},{discrete_v}) source={axis_source:?} stop={axis_stop} phase={phase:?} factors=(tp {:.2}, m {:.2}) -> {delta:?} at ({last_lx:.0},{last_ly:.0})",
                    factors.trackpad, factors.mouse
                );
            }
            let mut rebuild = false;
            if let Some(ctx) = self.inner.as_mut().unwrap().ui_context_mut() {
                ctx.ctrl_pressed = self.ctrl_pressed;
                ctx.shift_pressed = self.shift_pressed;
                ctx.alt_pressed = self.alt_pressed;
                ctx.logo_pressed = self.logo_pressed;
            }
            self.inner.as_mut().unwrap().handle_mouse_wheel(&delta, LogicalPosition::new(last_lx, last_ly), &mut rebuild);
            if rebuild {
                self.redraw = true;
            }
        }

        // App-driven window move/resize (non-standard CSD; see WindowAction):
        // executed with the serial of the most recent pointer press.
        if let Some(action) = self.inner.as_mut().unwrap().take_window_action() {
            if let (Some(ref window), Some(serial)) = (&self.window, self.last_press_serial) {
                let seat_owned = self.seats.first().cloned().or_else(|| self.seat_state.seats().next());
                if let Some(ref seat) = seat_owned {
                    match action {
                        WindowAction::Move => window.move_(seat, serial),
                        WindowAction::Resize(edge) => window.resize(seat, serial, edge),
                    }
                }
            }
        }
    }
}

impl<A: Application> KeyboardHandler for EngineState<A> {
    fn enter(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _keyboard: &wl_keyboard::WlKeyboard,
        _surface: &wl_surface::WlSurface,
        _serial: u32,
        _raw_modifiers: &[u32],
        _keysyms: &[xkeysym::Keysym],
    ) {
        let mut rebuild = false;
        self.inner.as_mut().unwrap().handle_focus_change(true, &mut rebuild);
        if rebuild {
            self.redraw = true;
        }
    }

    fn leave(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _keyboard: &wl_keyboard::WlKeyboard,
        _surface: &wl_surface::WlSurface,
        _serial: u32,
    ) {
        self.pressed_key = None;
        self.ctrl_pressed = false;
        self.shift_pressed = false;
        self.alt_pressed = false;
        let mut rebuild = false;
        self.inner.as_mut().unwrap().handle_focus_change(false, &mut rebuild);
        if rebuild {
            self.redraw = true;
        }
    }
    
    fn press_key(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _keyboard: &wl_keyboard::WlKeyboard,
        _serial: u32,
        event: smithay_client_toolkit::seat::keyboard::KeyEvent,
    ) {
        self.handle_key(event, ElementState::Pressed);
    }
    
    fn release_key(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _keyboard: &wl_keyboard::WlKeyboard,
        _serial: u32,
        event: smithay_client_toolkit::seat::keyboard::KeyEvent,
    ) {
        self.handle_key(event, ElementState::Released);
    }
    
    fn update_modifiers(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _keyboard: &wl_keyboard::WlKeyboard,
        _serial: u32,
        modifiers: smithay_client_toolkit::seat::keyboard::Modifiers,
        _layout: u32,
    ) {
        self.ctrl_pressed = modifiers.ctrl;
        self.shift_pressed = modifiers.shift;
        self.alt_pressed = modifiers.alt;
        self.logo_pressed = modifiers.logo;

        if let Some(ctx) = self.inner.as_mut().unwrap().ui_context_mut() {
            ctx.ctrl_pressed = self.ctrl_pressed;
            ctx.shift_pressed = self.shift_pressed;
            ctx.alt_pressed = self.alt_pressed;
            ctx.logo_pressed = self.logo_pressed;
        }
    }

    fn update_repeat_info(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _keyboard: &wl_keyboard::WlKeyboard,
        info: smithay_client_toolkit::seat::keyboard::RepeatInfo,
    ) {
        match info {
            smithay_client_toolkit::seat::keyboard::RepeatInfo::Repeat { rate, delay } => {
                // Store/expose delay/rate if required by the application
                let _ = (rate, delay);
            }
            smithay_client_toolkit::seat::keyboard::RepeatInfo::Disable => {}
        }
    }
}

impl<A: Application> EngineState<A> {
    /// The toolkit-wide undo/redo routing: a press matching the `undo` /
    /// `redo` chord goes to the focused widget first (`ContextAction::Undo`
    /// / `Redo` — a text box that is editing steps its own typing), then to
    /// the app's `Application::undo` / `redo`. Returns whether either took
    /// it; otherwise the key is dispatched as usual, so an app with its own
    /// scheme is undisturbed. Runs for repeats too — holding the chord walks
    /// the history like holding Backspace walks the text.
    /// The toolkit's Tab traversal, for apps that opt in
    /// (`Application::plate_navigation`): a bare Tab / Shift+Tab press moves
    /// keyboard focus to the next / previous plate or well. Returns whether it
    /// moved; otherwise the key is dispatched as usual.
    fn route_plate_navigation(&mut self, event: &KeyEvent, rebuild: &mut bool) -> bool {
        if event.state != ElementState::Pressed {
            return false;
        }
        // The group jump first (its chords carry ctrl); then a bare Tab.
        let group_next = crate::widget::match_key_shortcut(event, &self.group_next_chord);
        let group_prev = !group_next && crate::widget::match_key_shortcut(event, &self.group_prev_chord);
        let bare_tab = event.logical_key == Key::Named(NamedKey::Tab)
            && !self.ctrl_pressed
            && !self.alt_pressed
            && !self.logo_pressed;
        if !group_next && !group_prev && !bare_tab {
            return false;
        }
        let reverse = if bare_tab { self.shift_pressed } else { group_prev };
        let app = self.inner.as_mut().unwrap();
        if !app.plate_navigation() {
            return false;
        }
        let moved = app.ui_context_mut().is_some_and(|ctx| if bare_tab { ctx.focus_step(reverse) } else { ctx.focus_step_group(reverse) });
        if moved {
            app.focus_stepped();
            *rebuild = true;
        }
        moved
    }

    fn route_history_chord(&mut self, event: &KeyEvent, rebuild: &mut bool) -> bool {
        if event.state != ElementState::Pressed {
            return false;
        }
        let undo = crate::widget::match_key_shortcut(event, &self.undo_chord);
        let redo = !undo && crate::widget::match_key_shortcut(event, &self.redo_chord);
        if !undo && !redo {
            return false;
        }
        let app = self.inner.as_mut().unwrap();
        let action = if undo { crate::widget::ContextAction::Undo } else { crate::widget::ContextAction::Redo };
        if let Some(ctx) = app.ui_context_mut() {
            if ctx.focused_context_action(action) {
                *rebuild = true;
                return true;
            }
        }
        let taken = if undo { app.undo(rebuild) } else { app.redo(rebuild) };
        if taken {
            *rebuild = true;
        }
        taken
    }

    fn handle_key(&mut self, event: smithay_client_toolkit::seat::keyboard::KeyEvent, state: ElementState) {
        let logical_key = match event.keysym {
            xkeysym::Keysym::Escape => Key::Named(NamedKey::Escape),
            xkeysym::Keysym::Return => Key::Named(NamedKey::Enter),
            xkeysym::Keysym::BackSpace => Key::Named(NamedKey::Backspace),
            xkeysym::Keysym::Down => Key::Named(NamedKey::ArrowDown),
            xkeysym::Keysym::Up => Key::Named(NamedKey::ArrowUp),
            xkeysym::Keysym::Left => Key::Named(NamedKey::ArrowLeft),
            xkeysym::Keysym::Right => Key::Named(NamedKey::ArrowRight),
            // xkb reports Shift+Tab as ISO_Left_Tab; apps see plain Tab plus
            // the shift modifier, matching winit.
            xkeysym::Keysym::Tab | xkeysym::Keysym::ISO_Left_Tab => Key::Named(NamedKey::Tab),
            xkeysym::Keysym::Delete => Key::Named(NamedKey::Delete),
            xkeysym::Keysym::space => Key::Named(NamedKey::Space),
            xkeysym::Keysym::Page_Up => Key::Named(NamedKey::PageUp),
            xkeysym::Keysym::Page_Down => Key::Named(NamedKey::PageDown),
            xkeysym::Keysym::Home => Key::Named(NamedKey::Home),
            xkeysym::Keysym::End => Key::Named(NamedKey::End),
            xkeysym::Keysym::Super_L | xkeysym::Keysym::Super_R => Key::Named(NamedKey::Super),
            xkeysym::Keysym::Alt_L | xkeysym::Keysym::Alt_R => Key::Named(NamedKey::Alt),
            xkeysym::Keysym::Control_L | xkeysym::Keysym::Control_R => Key::Named(NamedKey::Control),
            xkeysym::Keysym::Shift_L | xkeysym::Keysym::Shift_R => Key::Named(NamedKey::Shift),
            xkeysym::Keysym::F5 => Key::Named(NamedKey::F5),
            _ => {
                // With Ctrl held, xkb's utf8 goes through the legacy control-character
                // transformation (ctrl+j = "\n", ctrl+a = 0x01, ...); the keysym is
                // untransformed, so prefer it there or ctrl+<letter> shortcuts can
                // never match their letter.
                if self.ctrl_pressed {
                    if let Some(ch) = event.keysym.key_char() {
                        Key::Character(ch.to_string())
                    } else if let Some(ref text) = event.utf8 {
                        Key::Character(text.clone())
                    } else {
                        return;
                    }
                } else if let Some(ref text) = event.utf8 {
                    Key::Character(text.clone())
                } else if let Some(ch) = event.keysym.key_char() {
                    Key::Character(ch.to_string())
                } else {
                    return;
                }
            }
        };

        let custom_event = KeyEvent {
            state,
            logical_key,
            text: event.utf8.clone(),
            repeat: false,
            ctrl: self.ctrl_pressed,
            shift: self.shift_pressed,
            alt: self.alt_pressed,
        };

        if state == ElementState::Pressed {
            if is_repeatable_key(&custom_event.logical_key) {
                self.pressed_key = Some(PressedKey {
                    logical_key: custom_event.logical_key.clone(),
                    text: custom_event.text.clone(),
                    first_pressed: Instant::now(),
                    last_repeated: Instant::now(),
                });
            } else {
                self.pressed_key = None;
            }
        } else if state == ElementState::Released {
            if let Some(ref pk) = self.pressed_key {
                if pk.logical_key == custom_event.logical_key {
                    self.pressed_key = None;
                }
            }
        }

        if let Some(ctx) = self.inner.as_mut().unwrap().ui_context_mut() {
            ctx.ctrl_pressed = self.ctrl_pressed;
            ctx.shift_pressed = self.shift_pressed;
            ctx.alt_pressed = self.alt_pressed;
            ctx.logo_pressed = self.logo_pressed;
        }

        // Escape dismisses the shared context menu before app dispatch — the
        // toolkit-wide default, mirroring the click-outside dismissal. Consumed:
        // while a menu is open, Escape means "close it", nothing else.
        if state == ElementState::Pressed
            && custom_event.logical_key == Key::Named(NamedKey::Escape)
            && crate::widget::context_menu::is_visible()
        {
            crate::widget::context_menu::hide();
            self.redraw = true;
            return;
        }

        let mut rebuild = false;
        if self.route_history_chord(&custom_event, &mut rebuild)
            || self.route_plate_navigation(&custom_event, &mut rebuild)
        {
            self.redraw = true;
            return;
        }
        if let Some(msg) = self.inner.as_mut().unwrap().handle_key_input(&custom_event, &mut rebuild) {
            let mut update_rebuild = false;
            self.inner.as_mut().unwrap().update(msg, &mut update_rebuild, &mut self.exit);
            if update_rebuild {
                rebuild = true;
            }
        }
        if rebuild {
            self.redraw = true;
        }
    }
}

impl<A: Application> wayland_client::Dispatch<wl_registry::WlRegistry, GlobalList, Self> for EngineState<A> {
    fn event(
        _state: &mut Self,
        _proxy: &wl_registry::WlRegistry,
        _event: wl_registry::Event,
        _data: &GlobalList,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {}
}

impl<A: Application> wayland_client::Dispatch<crate::protocol::zcce_inspector_v1::ZcceInspectorV1, ()> for EngineState<A> {
    fn event(
        _state: &mut Self,
        _proxy: &crate::protocol::zcce_inspector_v1::ZcceInspectorV1,
        _event: crate::protocol::zcce_inspector_v1::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {}
}

impl<A: Application> wayland_client::Dispatch<crate::protocol::cce_window_management_v1::zcce_window_manager_v1::ZcceWindowManagerV1, ()> for EngineState<A> {
    fn event(
        _state: &mut Self,
        _proxy: &crate::protocol::cce_window_management_v1::zcce_window_manager_v1::ZcceWindowManagerV1,
        _event: crate::protocol::cce_window_management_v1::zcce_window_manager_v1::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {}

    wayland_client::event_created_child!(
        EngineState<A>,
        crate::protocol::cce_window_management_v1::zcce_window_manager_v1::ZcceWindowManagerV1,
        [
            6 => (crate::protocol::cce_window_management_v1::zcce_window_v1::ZcceWindowV1, ()),
            7 => (crate::protocol::cce_window_management_v1::zcce_output_v1::ZcceOutputV1, ()),
            8 => (crate::protocol::cce_window_management_v1::zcce_seat_v1::ZcceSeatV1, ()),
        ]
    );
}

impl<A: Application> wayland_client::Dispatch<crate::protocol::cce_window_management_v1::zcce_window_v1::ZcceWindowV1, ()> for EngineState<A> {
    fn event(
        _state: &mut Self,
        _proxy: &crate::protocol::cce_window_management_v1::zcce_window_v1::ZcceWindowV1,
        _event: crate::protocol::cce_window_management_v1::zcce_window_v1::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {}
}

impl<A: Application> wayland_client::Dispatch<crate::protocol::cce_window_management_v1::zcce_output_v1::ZcceOutputV1, ()> for EngineState<A> {
    fn event(
        _state: &mut Self,
        _proxy: &crate::protocol::cce_window_management_v1::zcce_output_v1::ZcceOutputV1,
        _event: crate::protocol::cce_window_management_v1::zcce_output_v1::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {}
}

impl<A: Application> wayland_client::Dispatch<crate::protocol::cce_window_management_v1::zcce_seat_v1::ZcceSeatV1, ()> for EngineState<A> {
    fn event(
        _state: &mut Self,
        _proxy: &crate::protocol::cce_window_management_v1::zcce_seat_v1::ZcceSeatV1,
        _event: crate::protocol::cce_window_management_v1::zcce_seat_v1::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {}
}

impl<A: Application> wayland_client::Dispatch<crate::protocol::cce_window_management_v1::zcce_toplevel_v1::ZcceToplevelV1, ()> for EngineState<A> {
    fn event(
        state: &mut Self,
        _proxy: &crate::protocol::cce_window_management_v1::zcce_toplevel_v1::ZcceToplevelV1,
        event: crate::protocol::cce_window_management_v1::zcce_toplevel_v1::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        use crate::protocol::cce_window_management_v1::zcce_toplevel_v1::Event;
        if let Event::GridPatch { serial, x, y, width, height, scale } = event {
            // A newer patch supersedes an unconsumed older one.
            state.pending_grid_patch = Some((serial, x, y, width, height, scale));
            state.redraw = true;
        }
    }
}

delegate_compositor!(@<A: Application> EngineState<A>);
delegate_xdg_shell!(@<A: Application> EngineState<A>);
delegate_xdg_window!(@<A: Application> EngineState<A>);
delegate_layer!(@<A: Application> EngineState<A>);
delegate_shm!(@<A: Application> EngineState<A>);
delegate_seat!(@<A: Application> EngineState<A>);
delegate_pointer!(@<A: Application> EngineState<A>);
delegate_keyboard!(@<A: Application> EngineState<A>);
delegate_registry!(@<A: Application> EngineState<A>);
delegate_output!(@<A: Application> EngineState<A>);

impl<A: Application> wayland_client::Dispatch<wl_region::WlRegion, ()> for EngineState<A> {
    fn event(
        _state: &mut Self,
        _proxy: &wl_region::WlRegion,
        _event: wl_region::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {}
}

impl<A: Application> wayland_client::Dispatch<wl_callback::WlCallback, ()> for EngineState<A> {
    fn event(
        state: &mut Self,
        _proxy: &wl_callback::WlCallback,
        event: wl_callback::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        if let wl_callback::Event::Done { .. } = event {
            state.frame_callback_pending = false;
            if std::env::var("CCE_PRESENT_DEBUG").is_ok() {
                let t = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() % 100000;
                let waited = state.frame_callback_armed_at.map(|a| a.elapsed().as_millis()).unwrap_or(0);
                eprintln!("[vk] t={} frame-done (waited {}ms)", t, waited);
            }
        }
    }
}

impl<A: Application> wayland_client::Dispatch<ZwpPointerGesturesV1, ()> for EngineState<A> {
    fn event(
        _state: &mut Self,
        _proxy: &ZwpPointerGesturesV1,
        _event: zwp_pointer_gestures::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {}
}

impl<A: Application> wayland_client::Dispatch<ZwpPointerGesturePinchV1, ()> for EngineState<A> {
    fn event(
        state: &mut Self,
        _proxy: &ZwpPointerGesturePinchV1,
        event: zwp_pointer_gesture_pinch_v1::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        match event {
            zwp_pointer_gesture_pinch_v1::Event::Begin { .. } => {
                state.last_pinch_scale = 1.0;
            }
            zwp_pointer_gesture_pinch_v1::Event::Update { scale, .. } => {
                let scale_f32 = scale as f32;
                let factor = scale_f32 / state.last_pinch_scale;
                state.last_pinch_scale = scale_f32;

                let (px, py) = state.cursor_pos;
                let mut rebuild = false;

                // First offer the gesture as-is: apps with true pinch
                // surfaces (the designer's 3D viewport) consume it here at
                // 1:1 scale instead of through the wheel synthesis below.
                if state.inner.as_mut().unwrap().handle_pinch(factor, LogicalPosition::new(px, py), &mut rebuild) {
                    if rebuild {
                        state.redraw = true;
                    }
                    return;
                }

                // Calculate the y_delta for PixelDelta mapping.
                // Since cce-graph interprets factor = 1.0 + y_delta * 0.015, we reverse it:
                let y_delta = (factor - 1.0) / 0.015;
                let delta = MouseScrollDelta::PixelDelta(Position {
                    x: 0.0,
                    y: y_delta as f64,
                });

                if let Some(ctx) = state.inner.as_mut().unwrap().ui_context_mut() {
                    ctx.ctrl_pressed = true; // Force ctrl_pressed = true for the pinch event
                }
                // A synthesized delta, not a scroll gesture: no glide, no fling.
                crate::widget::scroll_motion::set_scroll_phase(crate::widget::ScrollPhase::Wheel);

                state.inner.as_mut().unwrap().handle_mouse_wheel(&delta, LogicalPosition::new(px, py), &mut rebuild);

                if let Some(ctx) = state.inner.as_mut().unwrap().ui_context_mut() {
                    ctx.ctrl_pressed = state.ctrl_pressed; // Restore original state
                }

                if rebuild {
                    state.redraw = true;
                }
            }
            zwp_pointer_gesture_pinch_v1::Event::End { .. } => {
                state.last_pinch_scale = 1.0;
            }
            _ => {}
        }
    }
}

/// Why a session's event loop stopped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SessionEnd {
    /// The app asked to exit.
    AppExit,
    /// The compositor connection died while the compositor itself may well be
    /// alive — a broken transport. The `Application` is intact and can be
    /// re-attached to a fresh connection.
    ConnectionLost,
    /// Nothing answered at the display socket: the compositor this app
    /// belonged to is gone. A deliberate exit unlinks the socket and a crash
    /// leaves it refusing; either way there is no session left to rejoin.
    NoCompositor,
}

/// What [`run`] does once a session has ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AfterSession {
    /// Leave the process-lifetime loop: run `on_exit` and quit.
    Exit,
    /// Sleep this long, then open a fresh session on the same `Application`.
    Reconnect(std::time::Duration),
}

/// How many consecutive failed reconnects before giving up. Reset once a
/// session has survived [`RECONNECT_RESET`], so a long-lived window that loses
/// its connection twice in a day still gets a full budget the second time.
const RECONNECT_ATTEMPTS: u32 = 8;
const RECONNECT_RESET: std::time::Duration = std::time::Duration::from_secs(10);

/// Decide whether a finished session is followed by another.
///
/// `lived` is how long the session that just ended lasted, `has_app` whether
/// an `Application` exists to carry over, and `attempt` the running count of
/// consecutive reconnects (reset here once a session outlives
/// [`RECONNECT_RESET`]).
///
/// Only a lost connection is retried, and only while the compositor is still
/// there to reconnect to. A reconnect is a repair of THIS session's transport
/// — the fd-exhaustion break `raise_fd_limit` documents — not a way to outlive
/// the compositor. When the connect itself fails the compositor has exited,
/// and it has already saved this window for restore: the next compositor
/// respawns the app from `state.json` on its own. A client that kept
/// retrying instead (the backoff below spans ~25s) reattached to that
/// successor beside the respawned copy, and every restore after a forced
/// exit or a crash came up with two of each cce-ui window. So the process
/// exits, as a Wayland client whose display went away always has.
fn after_session(
    end: SessionEnd,
    has_app: bool,
    lived: std::time::Duration,
    attempt: &mut u32,
) -> AfterSession {
    match end {
        SessionEnd::AppExit | SessionEnd::NoCompositor => AfterSession::Exit,
        SessionEnd::ConnectionLost => {
            // Nothing to preserve if we never got as far as building the
            // app — that is a failure to start, not a lost window.
            if !has_app {
                return AfterSession::Exit;
            }
            if lived > RECONNECT_RESET {
                *attempt = 0;
            }
            *attempt += 1;
            if *attempt > RECONNECT_ATTEMPTS {
                return AfterSession::Exit;
            }
            AfterSession::Reconnect(std::time::Duration::from_millis(
                100 * (1 << (*attempt).min(6)),
            ))
        }
    }
}

/// Raise this process's file-descriptor soft limit toward its hard limit.
///
/// A cce-ui client's fd usage is not bounded by anything the app controls.
/// Every dmabuf-feedback event the compositor sends carries a format-table
/// fd, and those arrive per surface whenever scanout candidacy changes —
/// entering the overview re-sends one for every window at once. Long-lived
/// windows sit at 700+ open fds in normal use, against a soft limit of 1024.
///
/// Crossing that limit does not fail politely. `recvmsg` drops the SCM_RIGHTS
/// payload when it cannot allocate descriptors, while still delivering the
/// message body — so libwayland hits a message whose fd never arrived,
/// reports "file descriptor expected", and the connection dies. That is
/// precisely the transport break [`run`] reconnects from below, at the cost
/// of a rebuilt window.
///
/// The compositor raises itself to 65536 for the same reason and then
/// deliberately restores the inherited limit for the programs it spawns
/// (cce-compositor `process.rs::cleanup_child`) — right for an arbitrary
/// child, far too low for a dmabuf-heavy Wayland client. So each client
/// raises its own, to the same ceiling.
fn raise_fd_limit() {
    unsafe {
        let mut lim: libc::rlimit = std::mem::zeroed();
        if libc::getrlimit(libc::RLIMIT_NOFILE, &mut lim) != 0 {
            return;
        }
        let want = std::cmp::min(65536, lim.rlim_max);
        if lim.rlim_cur >= want {
            return;
        }
        let raised = libc::rlimit { rlim_cur: want, rlim_max: lim.rlim_max };
        if libc::setrlimit(libc::RLIMIT_NOFILE, &raised) == 0 {
            log::info!("[window_runner] fd limit raised {} -> {}", lim.rlim_cur, want);
        } else {
            log::warn!("[window_runner] could not raise fd limit from {}", lim.rlim_cur);
        }
    }
}

/// Run an [`Application`] to completion, surviving loss of the compositor
/// connection.
///
/// A Wayland connection cannot be repaired once its transport state breaks — a
/// single dropped file descriptor on a dmabuf-feedback event is enough, and
/// libwayland then fails every dispatch with `EINVAL`. Exiting the process on
/// that error (the old behavior) threw away everything the window held: a
/// terminal's shell and scrollback, an editor's unsaved buffer.
///
/// So a connection is one *session*. Objects that belong to the connection —
/// the Wayland globals, the surface, the swapchain, the renderer — are rebuilt
/// per session. The things that carry user state outlive it: the `Application`
/// itself, the calloop loop, and the message channel. Keeping the **same
/// channel** matters as much as keeping the app: worker threads hold clones of
/// its `Sender` (cce-terminal's pty reader is the canonical case), and a fresh
/// channel would orphan them into a live-but-deaf process.
///
/// What is repaired is the transport, never the compositor: a reconnect only
/// goes through while the compositor that owned the lost session is still
/// listening. If the connect itself fails the compositor has exited, and the
/// process exits with it — see [`after_session`] for why staying alive there
/// duplicated every window on the next session restore.
///
/// Caveat: GPU resources belong to the renderer, so a rebuild re-runs
/// [`Application::renderer_init`]. Images uploaded outside it (e.g. in
/// [`Application::new`]) are not replayed into the new renderer — upload from
/// `renderer_init` if they must survive a reconnect.
pub fn run<A: Application>() {
    raise_fd_limit();

    // Outlives every session: worker threads hold this Sender, and the app's
    // own event sources are registered on this loop once.
    let (sender, channel) = calloop::channel::channel::<A::Message>();
    // Drop payloads come back from the per-drop reader threads (see
    // `backend::dnd`); registered once, like the app channel, because the
    // loop outlives a reconnect while the EngineState does not.
    let (drop_tx, drop_rx) =
        calloop::channel::channel::<crate::backend::dnd::DroppedData>();
    let mut event_loop = match EventLoop::try_new() {
        Ok(l) => l,
        Err(e) => {
            log::error!("[window_runner] cannot create event loop: {e}");
            return;
        }
    };
    event_loop
        .handle()
        .insert_source(channel, |event, _metadata, app_state: &mut EngineState<A>| {
            if let calloop::channel::Event::Msg(msg) = event {
                let mut rebuild = false;
                app_state.inner.as_mut().unwrap().update(msg, &mut rebuild, &mut app_state.exit);
                if rebuild {
                    app_state.redraw = true;
                }
            }
        })
        .unwrap();
    event_loop
        .handle()
        .insert_source(drop_rx, |event, _metadata, app_state: &mut EngineState<A>| {
            if let calloop::channel::Event::Msg(drop) = event {
                // The transfer is complete, so the source can be released now
                // — doing it any earlier costs the payload.
                if let Some(offer) = app_state.pending_drop_offer.take() {
                    offer.finish();
                    offer.destroy();
                }
                let mut rebuild = false;
                if let Some(app) = app_state.inner.as_mut() {
                    app.handle_drop(&drop.mime, &drop.bytes, drop.pos, &mut rebuild);
                }
                if rebuild {
                    app_state.redraw = true;
                }
            }
        })
        .unwrap();

    let mut app: Option<A> = None;
    let mut sources_registered = false;
    let mut attempt: u32 = 0;

    loop {
        let started = std::time::Instant::now();
        let (returned_app, end) =
            run_session(&mut event_loop, sender.clone(), drop_tx.clone(), app.take(), !sources_registered);
        app = returned_app;
        sources_registered = true;

        match after_session(end, app.is_some(), started.elapsed(), &mut attempt) {
            AfterSession::Exit => {
                match end {
                    SessionEnd::AppExit => {}
                    SessionEnd::NoCompositor if app.is_some() => log::warn!(
                        "[window_runner] compositor is gone; exiting (its successor restores the session itself)"
                    ),
                    SessionEnd::NoCompositor => {
                        log::error!("[window_runner] no compositor connection; giving up")
                    }
                    SessionEnd::ConnectionLost if app.is_some() => log::error!(
                        "[window_runner] connection lost; giving up after {} attempts",
                        attempt - 1
                    ),
                    SessionEnd::ConnectionLost => {
                        log::error!("[window_runner] no compositor connection; giving up")
                    }
                }
                break;
            }
            AfterSession::Reconnect(backoff) => {
                log::warn!(
                    "[window_runner] compositor connection lost; reconnecting in {backoff:?} (attempt {attempt})"
                );
                std::thread::sleep(backoff);
            }
        }
    }

    if let Some(mut app) = app {
        app.on_exit();
    }
    crate::process::cleanup_spawned_processes();
}

/// One connection's lifetime: connect, build the surface and renderer, pump
/// events until the app exits or the connection dies. Returns the
/// `Application` so the caller can hand it to the next session.
fn run_session<'l, A: Application>(
    event_loop: &mut EventLoop<'l, EngineState<A>>,
    sender: calloop::channel::Sender<A::Message>,
    drop_tx: calloop::channel::Sender<crate::backend::dnd::DroppedData>,
    existing_app: Option<A>,
    register_app_sources: bool,
) -> (Option<A>, SessionEnd) {
    let conn = match Connection::connect_to_env() {
        Ok(c) => c,
        Err(e) => {
            log::error!("[window_runner] cannot connect to compositor: {e}");
            return (existing_app, SessionEnd::NoCompositor);
        }
    };
    let (globals, mut event_queue) = match registry_queue_init(&conn) {
        Ok(v) => v,
        Err(e) => {
            log::error!("[window_runner] registry init failed: {e}");
            return (existing_app, SessionEnd::ConnectionLost);
        }
    };
    let qh = event_queue.handle();

    let compositor_state = CompositorState::bind(&globals, &qh).unwrap();
    let xdg_shell_state = XdgShell::bind(&globals, &qh).unwrap();
    let layer_shell_state = LayerShell::bind(&globals, &qh).ok();
    let shm_state = Shm::bind(&globals, &qh).unwrap();
    let seat_state = SeatState::new(&globals, &qh);
    let output_state = OutputState::new(&globals, &qh);

    let pointer_gestures: Option<ZwpPointerGesturesV1> = globals.bind(&qh, 1..=3, ()).ok();

    let mut engine_state = EngineState {
        data_device_manager: DataDeviceManagerState::bind(&globals, &qh).ok(),
        data_devices: Vec::new(),
        drag_mime: None,
        drag_pos: LogicalPosition::new(0.0, 0.0),
        drop_tx: Some(drop_tx),
        pending_drop_offer: None,
        applied_input_regions: None,
        registry_state: RegistryState::new(&globals),
        compositor_state,
        xdg_shell_state,
        layer_shell_state,
        shm_state,
        seat_state,
        output_state,
        seats: Vec::new(),
        pointer: None,
        keyboard: None,
        window: None,
        layer_surface: None,
        surface: None,
        inner: None,
        renderer: None,
        font_system: None,
        swash_cache: cosmic_text::SwashCache::new(),
        scale_factor: 1.0,
        committed_buffer_scale: 1,
        entered_outputs: Vec::new(),
        logical_width: 0.0,
        logical_height: 0.0,
        frame_logical: (0.0, 0.0),
        applied_margin: 0.0,
        overflow_was_active: false,
        sent_popover_region: None,
        exit: false,
        redraw: false,
        frame_callback_pending: false,
        frame_callback_armed_at: None,
        warm_until: None,
        extent_gate_skips: 0,
        first_configure_received: false,
        ctrl_pressed: false,
        undo_chord: crate::input::app_chord("undo", "ctrl+z"),
        redo_chord: crate::input::app_chord("redo", "ctrl+shift+z"),
        group_next_chord: crate::input::app_chord("focus_next_group", "ctrl+tab"),
        group_prev_chord: crate::input::app_chord("focus_prev_group", "ctrl+shift+tab"),
        shift_pressed: false,
        alt_pressed: false,
        logo_pressed: false,
        pressed_key: None,
        sender,
        current_cursor_icon: None,
        qh: qh.clone(),
        just_configured: false,
        pointer_gestures,
        pinch_gesture: None,
        cce_toplevel: None,
        pending_grid_patch: None,
        last_pinch_scale: 1.0,
        cursor_pos: (0.0, 0.0),
        last_press_serial: None,
        buttons_down: 0,
        dl_text_items: Vec::new(),
    };

    if let Err(e) = event_queue.roundtrip(&mut engine_state) {
        log::error!("[window_runner] initial roundtrip failed: {e}");
        return (existing_app, SessionEnd::ConnectionLost);
    }

    let scale = detect_scale_factor(&engine_state.output_state);
    engine_state.scale_factor = scale;
    crate::scale::set_scale_factor(scale as f32);
    crate::units::set_metric(crate::wayland::detect_metric(&engine_state.output_state, scale));

    // A reconnect re-attaches the SAME app: its state is the thing worth
    // saving, and `A::new` would both discard it and hand a fresh Sender to
    // worker threads that are still holding the original.
    let inner = match existing_app {
        Some(app) => app,
        None => A::new(&qh, engine_state.sender.clone()),
    };
    let settings = inner.settings();
    crate::scale::set_app_id(settings.app_id.clone());
    engine_state.logical_width = settings.width as f32;
    engine_state.logical_height = settings.height as f32;
    engine_state.inner = Some(inner);

    let surface = engine_state.compositor_state.create_surface(&qh);
    // A grid app's surface is pinned to scale 1: the patch's `scale` is
    // BUFFER px per virtual unit and already carries the output scale (the
    // patch manager folds it in), so adopting the output scale here would
    // square it — the client renders a doubled buffer and the compositor
    // downsamples it straight back into blur.
    if engine_state.inner.as_ref().unwrap().grid() {
        engine_state.scale_factor = 1.0;
    }
    // Forced-scale mode renders scaled-up into a buffer_scale-1 surface.
    let buffer_scale = if crate::scale::forced_scale().is_some()
        || engine_state.inner.as_ref().unwrap().grid()
    {
        1
    } else {
        scale as i32
    };
    surface.set_buffer_scale(buffer_scale);
    engine_state.committed_buffer_scale = buffer_scale;

    if settings.app_id.starts_with("cce-status") {
        let compositor = engine_state.compositor_state.wl_compositor();
        let region = compositor.create_region(&qh, ());
        region.add(0, 0, settings.width as i32, settings.height as i32);
        surface.set_input_region(Some(&region));
        region.destroy();
    }

    let layer_settings = engine_state.inner.as_ref().unwrap().layer();
    if let Some(ls) = layer_settings {
        let layer_shell = engine_state
            .layer_shell_state
            .as_ref()
            .expect("compositor does not support wlr-layer-shell");
        let layer_surface = layer_shell.create_layer_surface(
            &qh,
            surface.clone(),
            ls.layer,
            Some(ls.namespace.clone()),
            None,
        );
        layer_surface.set_anchor(ls.anchor);
        layer_surface.set_exclusive_zone(ls.exclusive_zone);
        layer_surface.set_keyboard_interactivity(ls.keyboard_interactivity);
        let (t, r, b, l) = ls.margin;
        layer_surface.set_margin(t, r, b, l);
        layer_surface.set_size(settings.width, settings.height);
        layer_surface.commit();
        engine_state.layer_surface = Some(layer_surface);
    } else {
        let window = engine_state.xdg_shell_state.create_window(surface.clone(), WindowDecorations::None, &qh);
        window.set_title(&settings.title);
        window.set_app_id(&settings.app_id);
        if settings.fullscreen {
            window.set_fullscreen(None);
        }
        if let Some((min_w, min_h)) = settings.min_size {
            window.set_min_size(Some((min_w, min_h)));
        }
        let wants_utility = engine_state.inner.as_ref().unwrap().utility();
        let wants_grid = engine_state.inner.as_ref().unwrap().grid();
        {
            // Bound for EVERY app now, not just utility/grid ones: the
            // toplevel also carries the popover-region hint (manager v7),
            // which any app with a dropdown wants. Role declarations go
            // BEFORE the initial commit so the mode is set by the time the
            // compositor maps the window. Version floors: set_utility
            // appeared at manager 5, the grid role at 6; the range tops at 7
            // so a newer compositor grants the hint and an older one simply
            // yields a lower-versioned toplevel — the hint send is gated on
            // version() >= 7 (send_popover_region), and on a pre-5
            // compositor the bind fails and the app runs plain.
            let version = if wants_grid { 6..=7 } else { 5..=7 };
            match globals.bind::<crate::protocol::cce_window_management_v1::zcce_window_manager_v1::ZcceWindowManagerV1, _, _>(&qh, version, ()) {
                Ok(cce_wm) => {
                    let toplevel = cce_wm.get_cce_toplevel(&surface, &qh, ());
                    if wants_utility {
                        toplevel.set_utility();
                    }
                    if wants_grid {
                        toplevel.set_grid();
                    }
                    engine_state.cce_toplevel = Some(toplevel);
                }
                Err(e) => {
                    log::warn!("[window_runner] cce window-management declaration unavailable: {e}");
                }
            }
        }
        window.commit();
        engine_state.window = Some(window);
    }
    engine_state.surface = Some(surface);

    // Overflow-margin mode: the surface (and so the GPU swapchain) is a rim
    // larger than the window frame on every side; geometry/input-region are
    // published per-resize.
    let rim = 2.0 * engine_state.inner.as_ref().unwrap().overflow_margin() as f32;
    engine_state.init_gpu(&conn, settings.width as f32 + rim, settings.height as f32 + rim);
    engine_state
        .inner
        .as_mut()
        .unwrap()
        .renderer_init(engine_state.renderer.as_mut().unwrap());

    let loop_handle = event_loop.handle();
    let wayland_token = match WaylandSource::new(conn.clone(), event_queue).insert(loop_handle.clone())
    {
        Ok(token) => token,
        Err(e) => {
            log::error!("[window_runner] cannot register the wayland source: {e}");
            return (engine_state.inner.take(), SessionEnd::ConnectionLost);
        }
    };

    // The app's own sources live on the persistent loop, so they are registered
    // once for the process — re-registering per session would double-deliver
    // every event on them.
    if register_app_sources {
        engine_state.inner.as_mut().unwrap().register_sources(&loop_handle);
    }

    const KEY_REPEAT_DELAY: std::time::Duration = std::time::Duration::from_millis(500);
    const KEY_REPEAT_INTERVAL: std::time::Duration = std::time::Duration::from_millis(50);

    /// Same switch as the renderer's present tracer, resolved once — this sits
    /// in the per-iteration path, so a `std::env::var` call here would be I/O
    /// on the loop that is under measurement.
    fn loop_debug() -> bool {
        static FLAG: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
        *FLAG.get_or_init(|| std::env::var_os("CCE_PRESENT_DEBUG").is_some())
    }

    /// Loop cadence while something is in motion: one tick per frame.
    const ACTIVE_DISPATCH: std::time::Duration = std::time::Duration::from_millis(16);

    /// Upper bound on an idle sleep. The loop is woken early by any Wayland
    /// event or calloop-channel message, so this only caps how long an
    /// app-side poll that bypasses both (see `Application::idle_poll_interval`)
    /// can wait. `CCE_UI_IDLE_MS` overrides it — `16` restores the old
    /// always-ticking loop for a bisect.
    fn idle_dispatch() -> std::time::Duration {
        static IDLE: std::sync::OnceLock<std::time::Duration> = std::sync::OnceLock::new();
        *IDLE.get_or_init(|| {
            std::env::var("CCE_UI_IDLE_MS")
                .ok()
                .and_then(|v| v.parse::<u64>().ok())
                .map(std::time::Duration::from_millis)
                .unwrap_or(IDLE_DISPATCH)
        })
    }

    /// Seconds after session start at which to inject a simulated connection
    /// loss, from `CCE_UI_FAULT_RECONNECT`. Resolved once: this is read from
    /// the per-iteration path.
    fn fault_reconnect_after() -> Option<std::time::Duration> {
        static AFTER: std::sync::OnceLock<Option<std::time::Duration>> =
            std::sync::OnceLock::new();
        *AFTER.get_or_init(|| {
            std::env::var("CCE_UI_FAULT_RECONNECT")
                .ok()
                .and_then(|v| v.parse::<f32>().ok())
                .map(std::time::Duration::from_secs_f32)
        })
    }

    let mut last_title = settings.title.clone();
    let mut last_tick = std::time::Instant::now();
    let mut end = SessionEnd::AppExit;
    let session_start = std::time::Instant::now();
    // The loop's cadence. ACTIVE while anything is in motion (a redraw
    // pending or just done, an animation, a held key, the post-activity
    // warm-down); otherwise the app's own poll interval or IDLE_DISPATCH.
    // Before 2026-09-11 this was a flat 16 ms whatever the state: every
    // cce-ui client woke 60 times a second forever — ~1200 wakeups/s across
    // a session's twenty clients — and each wake ran tick, desired_size,
    // title and margin checks for nothing.
    let mut next_timeout = ACTIVE_DISPATCH;
    let mut slept_idle = false;
    loop {
        // Frame callbacks arrive with a p50 of 0ms but a ~0.5s tail, while the
        // compositor's own trace shows it firing them within one or two vsyncs
        // of the arm. Tracing each iteration bisects that: if this loop keeps
        // turning at ~16ms all through a long wait, the event was not there to
        // read, and the delay is upstream rather than in dispatching it.
        let iter_start = if loop_debug() {
            Some(std::time::Instant::now())
        } else {
            None
        };
        if let Err(e) = event_loop.dispatch(next_timeout, &mut engine_state) {
            log::error!("[window_runner] event loop error, ending session: {e:?}");
            end = SessionEnd::ConnectionLost;
            break;
        }
        if let Some(start) = iter_start {
            let t = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis()
                % 100000;
            eprintln!(
                "[vk] t={} loop dispatch={}us pending_cb={}",
                t,
                start.elapsed().as_micros(),
                engine_state.frame_callback_pending
            );
        }
        // A protocol error kills the connection permanently, but it surfaces
        // through queue flushes whose errors calloop's WaylandSource swallows
        // (it only treats Io errors as fatal) — without this check the loop
        // spins forever on a dead display while wayland-backend re-prints the
        // error on every flush attempt.
        if let Some(perr) = conn.protocol_error() {
            log::error!("[window_runner] wayland protocol error, ending session: {perr}");
            end = SessionEnd::ConnectionLost;
            break;
        }
        // Fault injection for the reconnect path (`CCE_UI_FAULT_RECONNECT=<secs>`):
        // real connection loss is a rare race that cannot be provoked on demand,
        // so this drops the session exactly as a transport error would. One-shot
        // per process, so the app reconnects and then stays up.
        if let Some(after) = fault_reconnect_after() {
            static FIRED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
            if session_start.elapsed() >= after
                && !FIRED.swap(true, std::sync::atomic::Ordering::Relaxed)
            {
                log::warn!("[window_runner] CCE_UI_FAULT_RECONNECT: dropping the session");
                end = SessionEnd::ConnectionLost;
                break;
            }
        }
        if engine_state.exit {
            // The close dissolve. It is the COMPOSITOR that fades us — it
            // ramps our scene subtree's opacity, which takes the backdrop
            // blur, drop shadow and bevel down with the window; all this side
            // has to do is not vanish before it finishes. So keep the surface
            // mapped and the loop turning for exactly as long as the
            // compositor asked for, then leave. Dispatching (rather than
            // sleeping) keeps the connection pumped and lets any last
            // animation finish on screen while the window dissolves.
            let fade = crate::ipc::request_close_fade();
            if !fade.is_zero() {
                let until = std::time::Instant::now() + fade;
                loop {
                    let left = until.saturating_duration_since(std::time::Instant::now());
                    if left.is_zero() {
                        break;
                    }
                    if event_loop.dispatch(left.min(ACTIVE_DISPATCH), &mut engine_state).is_err() {
                        break;
                    }
                }
            }
            break;
        }

        let now = std::time::Instant::now();
        let mut dt = now.duration_since(last_tick).as_secs_f32();
        last_tick = now;
        if dt > 0.1 {
            dt = 0.1;
        }
        // Waking from an idle sleep: the interval is not animation time. An
        // animation an event just started must take its first step at frame
        // size, not leap 100 ms in one tick.
        if slept_idle {
            dt = dt.min(1.0 / 60.0);
        }

        let mut rebuild = false;
        let roster_ticks_before =
            engine_state.inner.as_mut().unwrap().ui_context_mut().map(|ctx| ctx.tick_count());
        engine_state.inner.as_mut().unwrap().tick(dt, &mut rebuild);
        if rebuild {
            engine_state.redraw = true;
        }
        // Tick the app's retained UiContext (widget tick_receivers — e.g. an
        // animating Dropdown popover) for apps that expose it — but only when
        // the app's own tick did not already do so this frame. Receivers
        // integrate `dt` (scroll glides, slider inertia), so the old
        // "double-ticking is harmless" assumption ran every glide at twice
        // its configured rate in apps that tick the context themselves.
        if let Some(ctx) = engine_state.inner.as_mut().unwrap().ui_context_mut() {
            if Some(ctx.tick_count()) == roster_ticks_before {
                if ctx.tick(dt) {
                    engine_state.redraw = true;
                }
            }
        }

        let just_configured = engine_state.just_configured;
        engine_state.just_configured = false;

        if !just_configured {
            if let Some((w, h)) = engine_state.inner.as_ref().unwrap().desired_size() {
                // desired_size is a window-frame size; the surface adds the
                // right/bottom overflow rim (0 for margin-less apps).
                let m = engine_state.inner.as_ref().unwrap().overflow_margin() as f32;
                let (sw, sh) = (w as f32 + m, h as f32 + m);
                if (engine_state.logical_width - sw).abs() > 0.001 || (engine_state.logical_height - sh).abs() > 0.001 {
                    engine_state.frame_logical = (w as f32, h as f32);
                    engine_state.applied_margin = m;
                    engine_state.resize(sw, sh);
                    engine_state.redraw = true;
                }
            }
        }

        // Overflow-margin drift (configure-sized apps): the rim can change at
        // runtime — a popover overhanging the window frame — so re-derive the
        // surface from the stored frame whenever the app's answer moves. While
        // the rim is live, re-publish geometry every loop: the input region
        // tracks the animating popover rects.
        {
            let m_now = engine_state.inner.as_ref().unwrap().overflow_margin() as f32;
            if (m_now - engine_state.applied_margin).abs() > 0.001 && engine_state.frame_logical.0 > 0.0 {
                engine_state.applied_margin = m_now;
                let (fw, fh) = engine_state.frame_logical;
                engine_state.resize(fw + m_now, fh + m_now);
                engine_state.redraw = true;
            }
            if engine_state.applied_margin > 0.0 || engine_state.overflow_was_active {
                engine_state.publish_window_geometry();
                engine_state.overflow_was_active = engine_state.applied_margin > 0.0;
            }
            engine_state.send_popover_region();
        }

        if let Some(ref mut pk) = engine_state.pressed_key {
            let now = std::time::Instant::now();
            if now.duration_since(pk.first_pressed) >= KEY_REPEAT_DELAY {
                if now.duration_since(pk.last_repeated) >= KEY_REPEAT_INTERVAL {
                    pk.last_repeated = now;
                    let custom_event = KeyEvent {
                        state: ElementState::Pressed,
                        logical_key: pk.logical_key.clone(),
                        text: pk.text.clone(),
                        repeat: true,
                        ctrl: engine_state.ctrl_pressed,
                        shift: engine_state.shift_pressed,
                        alt: engine_state.alt_pressed,
                    };

                    if let Some(ctx) = engine_state.inner.as_mut().unwrap().ui_context_mut() {
                        ctx.ctrl_pressed = engine_state.ctrl_pressed;
                        ctx.shift_pressed = engine_state.shift_pressed;
                        ctx.alt_pressed = engine_state.alt_pressed;
                        ctx.logo_pressed = engine_state.logo_pressed;
                    }

                    let mut key_rebuild = false;
                    if engine_state.route_history_chord(&custom_event, &mut key_rebuild)
                        || engine_state.route_plate_navigation(&custom_event, &mut key_rebuild)
                    {
                        engine_state.redraw = true;
                    } else if let Some(msg) = engine_state.inner.as_mut().unwrap().handle_key_input(&custom_event, &mut key_rebuild) {
                        let mut update_rebuild = false;
                        engine_state.inner.as_mut().unwrap().update(msg, &mut update_rebuild, &mut engine_state.exit);
                        if update_rebuild {
                            key_rebuild = true;
                        }
                    }
                    if key_rebuild {
                        engine_state.redraw = true;
                    }
                }
            }
        }
        let current_title = engine_state.inner.as_ref().unwrap().settings().title;
        if current_title != last_title {
            if let Some(ref window) = engine_state.window {
                window.set_title(&current_title);
                window.commit();
            }
            last_title = current_title;
        }

        // Frame-callback starvation fallback: the compositor only sends
        // frame-done for surfaces it actually renders, so a callback armed
        // while the window sat off-viewport (or the scene went static) may
        // never fire — and the vsync gate below then freezes the app forever
        // with a perfectly live event loop (input processes, state changes,
        // nothing repaints). If a redraw has been waiting on a callback well
        // past any real vsync interval, stop waiting and draw.
        //
        // Gated on the renderer's present mode: forcing a present past a
        // dead callback is only safe under MAILBOX (the present replaces the
        // queued buffer). Under FIFO the driver's throttle waits on the
        // previous present's frame event, so the forced present itself
        // blocks forever inside the driver — the exact freeze this fallback
        // exists to prevent. There the gate stays closed: pixels may stale
        // until the next frame-done/configure, but the loop stays alive.
        if engine_state.redraw
            && engine_state.frame_callback_pending
            && engine_state
                .renderer
                .as_ref()
                .is_some_and(|r| r.forced_present_safe())
            && engine_state
                .frame_callback_armed_at
                .is_none_or(|t| t.elapsed().as_millis() > 250)
        {
            engine_state.frame_callback_pending = false;
            if std::env::var("CCE_PRESENT_DEBUG").is_ok() {
                let t = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() % 100000;
                eprintln!("[vk] t={} starvation fallback fired (callback never came)", t);
            }
        }

        if engine_state.redraw {
            // Genuine dirt (input, app state, animation) extends the warm window;
            // warm-down renders below do NOT, so idle decays in one window.
            engine_state.warm_until =
                Some(std::time::Instant::now() + std::time::Duration::from_millis(200));
        }
        let mut rendered = false;
        if engine_state.redraw && !engine_state.frame_callback_pending {
            engine_state.redraw = false;
            if engine_state.first_configure_received {
                engine_state.render();
                rendered = true;
            }
        } else if !engine_state.redraw
            && !engine_state.frame_callback_pending
            && engine_state
                .warm_until
                .is_some_and(|t| std::time::Instant::now() < t)
        {
            // Warm-down re-render of the cached frame, paced by frame callbacks.
            if engine_state.first_configure_received {
                engine_state.render();
                rendered = true;
            }
        }

        // Anything still moving keeps the frame cadence; a frame callback
        // outstanding on its own does not (it arrives as an event) unless a
        // redraw is queued behind it, which is what the starvation fallback
        // above times. `redraw` still set here means the frame was withheld
        // (callback pending, or no configure yet) and must be retried soon.
        let warm = engine_state
            .warm_until
            .is_some_and(|t| std::time::Instant::now() < t);
        let busy = engine_state.redraw || rendered || warm || engine_state.pressed_key.is_some();
        next_timeout = if busy {
            ACTIVE_DISPATCH
        } else {
            let app_poll = engine_state.inner.as_ref().unwrap().idle_poll_interval();
            app_poll.map_or(idle_dispatch(), |d| d.min(idle_dispatch()))
        };
        slept_idle = !busy;
    }

    // Tear the session down: drop its Wayland source from the persistent loop
    // (leaving it would leak a dead source per reconnect), then hand the app
    // back before `engine_state` drops the renderer and the surface with it.
    // `on_exit` and process cleanup belong to the app's real exit, in `run`.
    loop_handle.remove(wayland_token);
    let app = engine_state.inner.take();
    drop(engine_state);
    (app, end)
}

// `all(test, debug_assertions)`: the function under test only exists in
// debug builds, so a `cargo test --release` must compile the module out too.
#[cfg(all(test, debug_assertions))]
mod near_roll_fallback_tests {
    use super::near_roll_fallback_reason;
    use crate::scene::layout::Rect;

    fn r(x: f32, y: f32, w: f32, h: f32) -> Rect {
        Rect { x, y, width: w, height: h }
    }

    const HOST: Rect = Rect { x: 0.0, y: 0.0, width: 800.0, height: 600.0 };
    const ROLL: f32 = 8.0;

    #[test]
    fn interior_carve_is_quiet() {
        // Well inside the deflated host: the overlay fallback is exact there.
        let carve = r(100.0, 100.0, 200.0, 100.0);
        assert_eq!(near_roll_fallback_reason(&carve, 6.0, &HOST, ROLL, &[], false), None);
    }

    #[test]
    fn shaded_region_reaching_the_roll_is_loud() {
        // Carve rect stops 3px short of the roll band, but its shaded region
        // (depth*0.5 + 2 = 5px) crosses in — the inflation must count.
        let carve = r(ROLL + 3.0, 100.0, 200.0, 100.0);
        assert_eq!(
            near_roll_fallback_reason(&carve, 6.0, &HOST, ROLL, &[], false),
            Some("the host's feature run is closed (another plate appended features since)")
        );
    }

    #[test]
    fn occlusion_is_named_before_run_contiguity() {
        let carve = r(2.0, 100.0, 200.0, 100.0);
        let occluder = r(150.0, 150.0, 100.0, 100.0);
        assert_eq!(
            near_roll_fallback_reason(&carve, 6.0, &HOST, ROLL, &[occluder], false),
            Some("a later plate overlaps the carve's shaded region")
        );
    }

    #[test]
    fn non_overlapping_later_plate_is_not_occlusion() {
        let carve = r(2.0, 100.0, 200.0, 100.0);
        let elsewhere = r(500.0, 400.0, 100.0, 100.0);
        assert_eq!(
            near_roll_fallback_reason(&carve, 6.0, &HOST, ROLL, &[elsewhere], false),
            Some("the host's feature run is closed (another plate appended features since)")
        );
    }

    #[test]
    fn budget_wins_over_every_other_reason() {
        let carve = r(2.0, 100.0, 200.0, 100.0);
        let occluder = r(150.0, 150.0, 100.0, 100.0);
        assert_eq!(
            near_roll_fallback_reason(&carve, 6.0, &HOST, ROLL, &[occluder], true),
            Some("the feature budget is full")
        );
    }
}

#[cfg(test)]
mod reconnect_tests {
    use super::{after_session, AfterSession, SessionEnd, RECONNECT_ATTEMPTS, RECONNECT_RESET};
    use std::time::Duration;

    const LONG: Duration = Duration::from_secs(60);
    const SHORT: Duration = Duration::from_millis(50);

    #[test]
    fn app_exit_ends_the_process() {
        let mut attempt = 0;
        assert_eq!(after_session(SessionEnd::AppExit, true, LONG, &mut attempt), AfterSession::Exit);
        assert_eq!(attempt, 0);
    }

    #[test]
    fn lost_transport_reconnects_with_backoff() {
        let mut attempt = 0;
        assert_eq!(
            after_session(SessionEnd::ConnectionLost, true, LONG, &mut attempt),
            AfterSession::Reconnect(Duration::from_millis(200))
        );
        assert_eq!(attempt, 1);
        assert_eq!(
            after_session(SessionEnd::ConnectionLost, true, SHORT, &mut attempt),
            AfterSession::Reconnect(Duration::from_millis(400))
        );
        assert_eq!(attempt, 2);
    }

    /// The compositor exited (its socket is unlinked, or refusing after a
    /// crash). It saved this window for restore, so the successor respawns
    /// the app itself; a client that waited for it reattached beside the
    /// respawned copy, and the restore came up with two of every window.
    #[test]
    fn compositor_gone_exits_instead_of_waiting_for_a_successor() {
        let mut attempt = 0;
        assert_eq!(
            after_session(SessionEnd::NoCompositor, true, LONG, &mut attempt),
            AfterSession::Exit
        );
        // Even mid-budget: a reconnect that finds nobody listening is the
        // compositor leaving, not another transport break.
        let mut attempt = 3;
        assert_eq!(
            after_session(SessionEnd::NoCompositor, true, SHORT, &mut attempt),
            AfterSession::Exit
        );
    }

    #[test]
    fn nothing_to_carry_over_gives_up() {
        let mut attempt = 0;
        assert_eq!(
            after_session(SessionEnd::ConnectionLost, false, SHORT, &mut attempt),
            AfterSession::Exit
        );
        assert_eq!(
            after_session(SessionEnd::NoCompositor, false, SHORT, &mut attempt),
            AfterSession::Exit
        );
    }

    #[test]
    fn budget_is_bounded_and_resets_after_a_long_session() {
        let mut attempt = 0;
        for _ in 0..RECONNECT_ATTEMPTS {
            assert!(matches!(
                after_session(SessionEnd::ConnectionLost, true, SHORT, &mut attempt),
                AfterSession::Reconnect(_)
            ));
        }
        assert_eq!(
            after_session(SessionEnd::ConnectionLost, true, SHORT, &mut attempt),
            AfterSession::Exit
        );
        // A session that outlived the reset window earns a fresh budget.
        assert_eq!(
            after_session(SessionEnd::ConnectionLost, true, RECONNECT_RESET + SHORT, &mut attempt),
            AfterSession::Reconnect(Duration::from_millis(200))
        );
        assert_eq!(attempt, 1);
    }

    #[test]
    fn backoff_caps_at_six_point_four_seconds() {
        let mut attempt = 6;
        assert_eq!(
            after_session(SessionEnd::ConnectionLost, true, SHORT, &mut attempt),
            AfterSession::Reconnect(Duration::from_millis(6400))
        );
        assert_eq!(
            after_session(SessionEnd::ConnectionLost, true, SHORT, &mut attempt),
            AfterSession::Reconnect(Duration::from_millis(6400))
        );
    }
}
