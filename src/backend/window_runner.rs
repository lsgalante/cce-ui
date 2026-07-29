use std::time::Instant;
use smithay_client_toolkit::{
    compositor::{CompositorHandler, CompositorState},
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
            XdgShell,
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
use glyphon::{
    FontSystem,
    TextBounds, Buffer, Attrs, Metrics,
};
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

    let (sans_fallback, serif_fallback, mono_fallback, _, _, _, _) = crate::layout::read_preferred_fonts();

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
                        glyphon::Family::Name(cased)
                    } else {
                        glyphon::Family::Name(crate::layout::get_system_monospace_font())
                    }
                } else {
                    glyphon::Family::Name(crate::layout::get_system_monospace_font())
                }
            }
            "sans-serif" => {
                if !sans_fallback.is_empty() {
                    if let Some(ref cased) = resolved_storage {
                        glyphon::Family::Name(cased)
                    } else {
                        glyphon::Family::SansSerif
                    }
                } else {
                    glyphon::Family::SansSerif
                }
            }
            "serif" => {
                if !serif_fallback.is_empty() {
                    if let Some(ref cased) = resolved_storage {
                        glyphon::Family::Name(cased)
                    } else {
                        glyphon::Family::Serif
                    }
                } else {
                    glyphon::Family::Serif
                }
            }
            name => glyphon::Family::Name(name),
        }
    } else {
        if !sans_fallback.is_empty() {
            if let Some(ref cased) = resolved_sans {
                glyphon::Family::Name(cased)
            } else {
                glyphon::Family::SansSerif
            }
        } else {
            glyphon::Family::SansSerif
        }
    };
    attrs = attrs.family(family);
    if text_attrs.italic {
        attrs = attrs.style(glyphon::Style::Italic);
    }
    if let Some(w) = text_attrs.weight {
        attrs = attrs.weight(glyphon::Weight(w));
    }
    buf.set_text(fs, text, attrs, glyphon::Shaping::Advanced);
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
        AlignH::Left => glyphon::cosmic_text::Align::Left,
        AlignH::Center => glyphon::cosmic_text::Align::Center,
        AlignH::Right => glyphon::cosmic_text::Align::Right,
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
            verts.extend_from_slice(&line_vertices(x1, y1, x2, y2, thickness, sw, sh, c));
            let clip_circle = [0.0, 0.0, 0.0];
            verts.extend(circle_vertices(x2, y2, thickness / 2.0, sw, sh, c, 16, clip_circle));
        }
        LineCap::Flat => {
            verts.extend_from_slice(&line_vertices(x1, y1, x2, y2, thickness, sw, sh, c));
        }
    }
    
    verts
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

    // Corner rendering
    let segments = 16;

    // Top-Left
    if r_tl > 0.1 {
        let cx = x + r_tl;
        let cy = y + r_tl;
        let start = std::f32::consts::PI;
        let end = 1.5 * std::f32::consts::PI;
        for i in 0..segments {
            let theta1 = start + (i as f32) * (end - start) / (segments as f32);
            let theta2 = start + ((i + 1) as f32) * (end - start) / (segments as f32);
            
            let (c1, s1) = superellipse_pt(theta1, corner_e);
            let (c2, s2) = superellipse_pt(theta2, corner_e);
            let x0 = clamp_x(cx);
            let y0 = clamp_y(cy);
            let x1 = clamp_x(cx + r_tl * c1);
            let y1 = clamp_y(cy + r_tl * s1);
            let x2 = clamp_x(cx + r_tl * c2);
            let y2 = clamp_y(cy + r_tl * s2);
            
            let ndc_x0 = (x0 / sw) * 2.0 - 1.0;
            let ndc_y0 = 1.0 - (y0 / sh) * 2.0;
            let ndc_x1 = (x1 / sw) * 2.0 - 1.0;
            let ndc_y1 = 1.0 - (y1 / sh) * 2.0;
            let ndc_x2 = (x2 / sw) * 2.0 - 1.0;
            let ndc_y2 = 1.0 - (y2 / sh) * 2.0;
            
            out.push(Vertex { position: [ndc_x0, ndc_y0], color, clip_circle });
            out.push(Vertex { position: [ndc_x1, ndc_y1], color, clip_circle });
            out.push(Vertex { position: [ndc_x2, ndc_y2], color, clip_circle });
        }
        if mid_x0 > r_tl {
            push_quad(out, x + r_tl, y, mid_x0 - r_tl, r_tl);
        }
    } else {
        push_quad(out, x, y, mid_x0, 0.0);
    }

    // Top-Right
    if r_tr > 0.1 {
        let cx = x + ww - r_tr;
        let cy = y + r_tr;
        let start = 1.5 * std::f32::consts::PI;
        let end = 2.0 * std::f32::consts::PI;
        for i in 0..segments {
            let theta1 = start + (i as f32) * (end - start) / (segments as f32);
            let theta2 = start + ((i + 1) as f32) * (end - start) / (segments as f32);
            
            let (c1, s1) = superellipse_pt(theta1, corner_e);
            let (c2, s2) = superellipse_pt(theta2, corner_e);
            let x0 = clamp_x(cx);
            let y0 = clamp_y(cy);
            let x1 = clamp_x(cx + r_tr * c1);
            let y1 = clamp_y(cy + r_tr * s1);
            let x2 = clamp_x(cx + r_tr * c2);
            let y2 = clamp_y(cy + r_tr * s2);
            
            let ndc_x0 = (x0 / sw) * 2.0 - 1.0;
            let ndc_y0 = 1.0 - (y0 / sh) * 2.0;
            let ndc_x1 = (x1 / sw) * 2.0 - 1.0;
            let ndc_y1 = 1.0 - (y1 / sh) * 2.0;
            let ndc_x2 = (x2 / sw) * 2.0 - 1.0;
            let ndc_y2 = 1.0 - (y2 / sh) * 2.0;
            
            out.push(Vertex { position: [ndc_x0, ndc_y0], color, clip_circle });
            out.push(Vertex { position: [ndc_x1, ndc_y1], color, clip_circle });
            out.push(Vertex { position: [ndc_x2, ndc_y2], color, clip_circle });
        }
        if ww - mid_x1 > r_tr {
            push_quad(out, x + mid_x1, y, ww - mid_x1 - r_tr, r_tr);
        }
    } else {
        push_quad(out, x + mid_x1, y, ww - mid_x1, 0.0);
    }

    // Bottom-Right
    if r_br > 0.1 {
        let cx = x + ww - r_br;
        let cy = y + h - r_br;
        let start = 0.0;
        let end = 0.5 * std::f32::consts::PI;
        for i in 0..segments {
            let theta1 = start + (i as f32) * (end - start) / (segments as f32);
            let theta2 = start + ((i + 1) as f32) * (end - start) / (segments as f32);
            
            let (c1, s1) = superellipse_pt(theta1, corner_e);
            let (c2, s2) = superellipse_pt(theta2, corner_e);
            let x0 = clamp_x(cx);
            let y0 = clamp_y(cy);
            let x1 = clamp_x(cx + r_br * c1);
            let y1 = clamp_y(cy + r_br * s1);
            let x2 = clamp_x(cx + r_br * c2);
            let y2 = clamp_y(cy + r_br * s2);
            
            let ndc_x0 = (x0 / sw) * 2.0 - 1.0;
            let ndc_y0 = 1.0 - (y0 / sh) * 2.0;
            let ndc_x1 = (x1 / sw) * 2.0 - 1.0;
            let ndc_y1 = 1.0 - (y1 / sh) * 2.0;
            let ndc_x2 = (x2 / sw) * 2.0 - 1.0;
            let ndc_y2 = 1.0 - (y2 / sh) * 2.0;
            
            out.push(Vertex { position: [ndc_x0, ndc_y0], color, clip_circle });
            out.push(Vertex { position: [ndc_x1, ndc_y1], color, clip_circle });
            out.push(Vertex { position: [ndc_x2, ndc_y2], color, clip_circle });
        }
        if ww - mid_x1 > r_br {
            push_quad(out, x + mid_x1, y + h - r_br, ww - mid_x1 - r_br, r_br);
        }
    } else {
        push_quad(out, x + mid_x1, y + h, ww - mid_x1, 0.0);
    }

    // Bottom-Left
    if r_bl > 0.1 {
        let cx = x + r_bl;
        let cy = y + h - r_bl;
        let start = 0.5 * std::f32::consts::PI;
        let end = std::f32::consts::PI;
        for i in 0..segments {
            let theta1 = start + (i as f32) * (end - start) / (segments as f32);
            let theta2 = start + ((i + 1) as f32) * (end - start) / (segments as f32);
            
            let (c1, s1) = superellipse_pt(theta1, corner_e);
            let (c2, s2) = superellipse_pt(theta2, corner_e);
            let x0 = clamp_x(cx);
            let y0 = clamp_y(cy);
            let x1 = clamp_x(cx + r_bl * c1);
            let y1 = clamp_y(cy + r_bl * s1);
            let x2 = clamp_x(cx + r_bl * c2);
            let y2 = clamp_y(cy + r_bl * s2);
            
            let ndc_x0 = (x0 / sw) * 2.0 - 1.0;
            let ndc_y0 = 1.0 - (y0 / sh) * 2.0;
            let ndc_x1 = (x1 / sw) * 2.0 - 1.0;
            let ndc_y1 = 1.0 - (y1 / sh) * 2.0;
            let ndc_x2 = (x2 / sw) * 2.0 - 1.0;
            let ndc_y2 = 1.0 - (y2 / sh) * 2.0;
            
            out.push(Vertex { position: [ndc_x0, ndc_y0], color, clip_circle });
            out.push(Vertex { position: [ndc_x1, ndc_y1], color, clip_circle });
            out.push(Vertex { position: [ndc_x2, ndc_y2], color, clip_circle });
        }
        if mid_x0 > r_bl {
            push_quad(out, x + r_bl, y + h - r_bl, mid_x0 - r_bl, r_bl);
        }
    } else {
        push_quad(out, x, y + h, mid_x0, 0.0);
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
/// *modulation* instead of a resolved surface color is what lets bevels compose — a step
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
    let cap = ww.min(h) * 0.5;
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
/// Emitted as the same two-pass white/black overlays as the bevels (see
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
pub fn tessellate_display_list(
    dl: &crate::scene::paint::DisplayList,
    sw: f32,
    sh: f32,
    scale: f32,
) -> (Vec<Vertex>, Vec<DlBatch>, Vec<DlImage>, Vec<[f32; 12]>) {
    use crate::scene::paint::{Cap, Prim};
    let mut verts: Vec<Vertex> = Vec::new();
    let mut batches: Vec<DlBatch> = Vec::new();
    let mut images: Vec<DlImage> = Vec::new();
    // Carves CSG'd into plates (see Frame2D::plate_features), plus the plate
    // they group into: the most recent Plate/Bevel batch, provided only Text
    // and Image prims (which draw through separate paths anyway) intervene.
    let mut features: Vec<[f32; 12]> = Vec::new();
    let mut last_plate: Option<(usize, crate::scene::layout::Rect)> = None;

    // SDF-lit plate path (shader2d's plate branch) vs the legacy banded vertex
    // shading, plus the frame-constant lighting inputs it pushes per plate.
    let shader_plates = crate::layout::bevel_shader();
    let plate_light = {
        let az = crate::layout::light_source_position();
        let el = std::f32::consts::FRAC_PI_4; // light elevation above the screen plane
        [az.cos() * el.cos(), -az.sin() * el.cos(), el.sin()]
    };
    // [shading strength (1.0 at the default bevel_depth), specular strength,
    // shininess, curvature/AO strength] — the plastic material. Curvature is
    // kept near the raised path's crest amplitude: the recess shoulder's
    // brightening lands on the same pixels as its specular line, and the two
    // stack — at 0.5 the step read several times hotter than a plate roll.
    let plate_mat = [crate::layout::bevel_depth() / 0.15, 0.4, 24.0, 0.2];

    for item in &dl.items {
        let start = verts.len() as u32;
        let mut plate: Option<crate::vk::PlatePush> = None;
        let mut made_plate: Option<crate::scene::layout::Rect> = None;
        // Blur-behind marker: a Plate/Bevel whose fill alpha is negative asks
        // the renderer to snapshot the frame-so-far before it draws.
        let blur_behind = matches!(
            &item.prim,
            crate::scene::paint::Prim::Bevel { color, .. }
            | crate::scene::paint::Prim::Plate { color, .. } if color[3] < 0.0
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
            Prim::Quad { rect, color } => {
                verts.extend(quad_vertices(rect.x, rect.y, rect.width, rect.height, sw, sh, *color));
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
            Prim::Bevel { rect, radii, color, depth, tint } if shader_plates => {
                // SDF-lit raised plate: one cover quad; the shader owns fill,
                // roll shading, corners, and silhouette AA. Nominal corner
                // radii (scale_corners false): a Bevel is a WIDGET-scale plate
                // whose silhouette must match the nominal-radius squircles of
                // the controls around it — only window-scale `Plate`s get the
                // curvature-matched span.
                verts.extend(quad_vertices(rect.x, rect.y, rect.width, rect.height, sw, sh, *color));
                let mut p = plate_push_raised(rect, *radii, *depth, scale, plate_light, plate_mat, false);
                p.specular_tint = [tint[0], tint[1], tint[2], 0.0];
                plate = Some(p);
                made_plate = Some(*rect);
            }
            Prim::Plate { rect, radii, color, depth } if shader_plates => {
                // Same lit-plate branch; the cover quad is the exact rect so the
                // silhouette and the compositor's rounded window corners agree.
                verts.extend(quad_vertices(rect.x, rect.y, rect.width, rect.height, sw, sh, *color));
                plate = Some(plate_push_raised(rect, *radii, *depth, scale, plate_light, plate_mat, true));
                made_plate = Some(*rect);
            }
            Prim::Recess { rect, radii, depth, edges, .. }
            | Prim::Boss { rect, radii, depth, edges }
            | Prim::Ridge { rect, radii, depth, edges }
                if shader_plates =>
            {
                let tint = match &item.prim {
                    Prim::Recess { tint, .. } => *tint,
                    _ => None,
                };
                // Recess carves down into the surface; Boss raises a plateau out
                // of it (same machinery, depth sign flipped); Ridge is a raised
                // rim straddling the boundary (its own overlay profile — never
                // grouped, the CSG features only model monotonic steps).
                let mode = match &item.prim {
                    Prim::Boss { .. } => 3.0f32,
                    Prim::Ridge { .. } => 4.0,
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
                // (section wells, rocker halves) are exactly these.
                // A tinted carve also never groups: a CSG feature is geometry only,
                // so the tint could only land on the whole plate's specular.
                let full_ring = *edges == (true, true, true, true);
                if let Some((bi, prect)) = last_plate.filter(|_| mode < 3.5 && full_ring && tint.is_none()) {
                    let inside = rect.x >= prect.x - 0.5
                        && rect.y >= prect.y - 0.5
                        && rect.x + rect.width <= prect.x + prect.width + 0.5
                        && rect.y + rect.height <= prect.y + prect.height + 0.5;
                    if inside && features.len() < crate::vk::MAX_PLATE_FEATURES {
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
                        // Depth saturates at the DE's nominal roll width: a wall
                        // wider than the plate's own perimeter roll spreads that
                        // same step over the longer run — a softer transition —
                        // instead of cutting proportionally deeper (which would
                        // keep the wall just as steep no matter how wide it got).
                        let k_mag = RECESS_DEPTH_RATIO * t_px.min(crate::layout::bevel_width() * scale);
                        // Negative depth = raised (Boss); the shader's summed
                        // slope vectors and curvature sign follow it.
                        let k_px = if raised { -k_mag } else { k_mag };
                        if let Some(p) = batches[bi].plate.as_mut() {
                            if p.host[1] == 0.0 {
                                p.host[0] = features.len() as f32;
                            }
                            p.host[1] += 1.0;
                        }
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
                let mut p = plate_push_raised(&sdf_rect, *radii, *depth, scale, plate_light, plate_mat, false);
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
            Prim::Bevel { rect, radii, color, depth, tint: _ } => {
                // Full-size fill: the lip is now a shading overlay, not a paint of the
                // outer ring, so the fill must cover the whole rect (the old inset fill
                // would leave the ring showing whatever lay beneath).
                let corners = crate::widget::CornerRadii {
                    top_left: radii.0, top_right: radii.1,
                    bottom_right: radii.2, bottom_left: radii.3,
                };
                push_rounded_rect_vertices_corners(rect.x, rect.y, rect.width, rect.height, corners, sw, sh, *color, no, None, &mut verts);
                push_plate_bevel_vertices(rect.x, rect.y, rect.width, rect.height, radii.0, *depth, sw, sh, *color, no, &mut verts);
            }
            Prim::Plate { rect, radii, color, depth } => {
                // Fill at full size (no inset — see Prim::Plate), then light the face,
                // then roll the perimeter. The lip rides on top of the fill's outer band
                // rather than replacing it, so the plate's silhouette and the
                // compositor's rounded window corners still agree exactly.
                let corners = crate::widget::CornerRadii {
                    top_left: radii.0, top_right: radii.1,
                    bottom_right: radii.2, bottom_left: radii.3,
                };
                push_rounded_rect_vertices_corners(
                    rect.x, rect.y, rect.width, rect.height, corners, sw, sh, *color, no, None, &mut verts,
                );
                push_plate_face_vertices(rect.x, rect.y, rect.width, rect.height, sw, sh, no, &mut verts);
                push_bevel_edge_vertices_radii(
                    rect.x, rect.y, rect.width, rect.height, *radii, *depth,
                    sw, sh, *color, no, 1.0, &mut verts,
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
            Prim::Boss { rect, radii, depth, edges } => {
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
            Prim::Arc { cx, cy, radius, thickness, start: sa, end: ea, color } => {
                push_arc_background_vertices(*cx, *cy, *radius, *thickness, *sa, *ea, sw, sh, *color, segs(*radius), no, &mut verts);
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
                verts.extend(circle_vertices(*cx, *cy, *radius, sw, sh, *color, segs(*radius), no));
            }
            Prim::Sphere { cx, cy, radius, color } if shader_plates => {
                // A hemisphere lit per pixel by the plate branch (mode 5): one
                // cover quad, its own never-merged batch. The quad overhangs
                // the disc by 1px for the shader's silhouette anti-aliasing.
                let d = *radius + 1.0;
                verts.extend(quad_vertices(cx - d, cy - d, 2.0 * d, 2.0 * d, sw, sh, *color));
                plate = Some(crate::vk::PlatePush {
                    // Center + radius in physical px; the SDF box machinery is
                    // unused in this mode, so .w is free.
                    rect: [cx * scale, cy * scale, radius * scale, 0.0],
                    radii: [0.0; 4],
                    light: [plate_light[0], plate_light[1], plate_light[2], 0.0],
                    material: plate_mat,
                    host: [0.0; 4],
                    specular_tint: [1.0, 1.0, 1.0, 0.0],
                    mode: 5.0,
                    shape: 2.0,
                });
            }
            Prim::Sphere { cx, cy, radius, color } => {
                // Legacy path: the flat disc, exactly a Circle.
                verts.extend(circle_vertices(*cx, *cy, *radius, sw, sh, *color, segs(*radius), no));
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
            last_plate = None;
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
        batches.push(DlBatch { scissor: item.clip, clip_rrect: item.clip_rrect, start, end, plate, blur_behind });
        if let Some(prect) = made_plate {
            last_plate = Some((batches.len() - 1, prect));
        }
    }

    (verts, batches, images, features)
}

/// A carve's depth as a fraction of its transition width — must match the
/// shader's `RECESS_DEPTH` (used by the mode-2 overlay fallback).
const RECESS_DEPTH_RATIO: f32 = 0.6;

/// The push-constant block for a raised SDF-lit plate over `rect` (logical px in,
/// physical px out). Corner radii clamp to the half-extent cap the SDF needs.
fn plate_push_raised(
    rect: &crate::scene::layout::Rect,
    radii: (f32, f32, f32, f32),
    width: f32,
    scale: f32,
    light: [f32; 3],
    material: [f32; 4],
    scale_corners: bool,
) -> crate::vk::PlatePush {
    let cap = rect.width.min(rect.height) * 0.5;
    let shape = crate::layout::corner_shape();
    // For PLATES (`scale_corners`), widen the corner span by the
    // curvature-match factor (see `layout::corner_span_factor`): the diagonal
    // curvature radius equals the configured radius, the corner reads as the
    // same size as a circular one, and every roll inset ≤ r stays crease-free
    // (past the diagonal curvature radius the offset curve the specular band
    // follows creases into a visible square corner). Widget-scale overlay
    // reliefs (recess/boss/ridge fallbacks) pass false: their radii must MATCH
    // the nominal-radius squircles of the widget silhouettes around them, and
    // at their few-px roll widths the offset crease is subpixel.
    let rscale = if scale_corners { crate::layout::corner_span_factor() } else { 1.0 };
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
    let top_room = crate::widget::label_offset(target_w);
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
        let top = crate::widget::label_offset(target_w);
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
    let top_room = crate::widget::label_offset(target_w);
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
        let top = crate::widget::label_offset(target_w);
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
    for i in 0..segments {
        let theta1 = start_angle + (i as f32) * (end_angle - start_angle) / (segments as f32);
        let theta2 = start_angle + ((i + 1) as f32) * (end_angle - start_angle) / (segments as f32);
        
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
        
        out.push(Vertex { position: [ndc_x0, ndc_y0], color, clip_circle });
        out.push(Vertex { position: [ndc_x1, ndc_y1], color, clip_circle });
        out.push(Vertex { position: [ndc_x2, ndc_y2], color, clip_circle });
        
        out.push(Vertex { position: [ndc_x0, ndc_y0], color, clip_circle });
        out.push(Vertex { position: [ndc_x2, ndc_y2], color, clip_circle });
        out.push(Vertex { position: [ndc_x3, ndc_y3], color, clip_circle });
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
    fn update(&mut self, msg: Self::Message, needs_rebuild: &mut bool, exit: &mut bool);
    fn tick(&mut self, dt: f32, needs_rebuild: &mut bool);
    /// On-top overlay quads drawn after the display list and its text (e.g. the status bar's
    /// tray-hover highlights). Deliberately separate from the single paint path.
    fn overlay_quads(&mut self, _quads: &mut Vec<(f32, f32, f32, f32, [f32; 4])>, _size: LogicalSize, _scale: f64) {}
    fn input_regions(&self) -> Option<Vec<(i32, i32, i32, i32)>> {
        None
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
    /// `Backplate` is dissolved (Phase 6), so the default is "no" — apps that want
    /// drag-anywhere override this with `ctx.drag_allowed_at(px, py)`.
    fn is_movable_backplate_at(&self, _px: f32, _py: f32) -> bool {
        false
    }
    
    fn clear_color(&self) -> [f32; 4] {
        [0.0, 0.0, 0.0, 0.0]
    }

    fn register_sources(&mut self, _handle: &calloop::LoopHandle<'_, EngineState<Self>>) {}

    fn adjust_size(&self, width: f32, height: f32) -> (f32, f32) {
        (width, height)
    }
    
    fn handle_pointer_move(&mut self, pos: LogicalPosition, needs_rebuild: &mut bool);
    fn handle_mouse_input(&mut self, button: MouseButton, state: ElementState, pos: LogicalPosition, needs_rebuild: &mut bool) -> Option<Self::Message>;
    fn handle_mouse_wheel(&mut self, delta: &MouseScrollDelta, pos: LogicalPosition, needs_rebuild: &mut bool);
    fn handle_key_input(&mut self, event: &KeyEvent, needs_rebuild: &mut bool) -> Option<Self::Message>;

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

    /// Opt in to render the display list's `Prim::Text` items through the glyphon pass
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
    /// titlebar move band, the movable-backplate drag regions, and — when
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
    /// [`is_movable_backplate_at`](Application::is_movable_backplate_at)); nothing is
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
    pub swash_cache: glyphon::SwashCache,
    
    pub scale_factor: f64,
    /// The buffer scale last sent to the surface. Updated in [`Self::render`],
    /// paired with the present that commits a matching-size buffer — never on
    /// the scale event itself, which races in-flight presents of old buffers.
    pub committed_buffer_scale: i32,
    pub logical_width: f32,
    pub logical_height: f32,
    
    pub exit: bool,
    pub redraw: bool,
    pub frame_callback_pending: bool,
    pub first_configure_received: bool,
    pub ctrl_pressed: bool,
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
    pub last_pinch_scale: f32,
    pub cursor_pos: (f32, f32),
    /// Serial of the most recent pointer press, kept for
    /// [`Application::take_window_action`] move/resize grabs.
    pub last_press_serial: Option<u32>,
    /// This frame's display-list text, shaped and held here so the glyphon `TextArea`s built
    /// in the render pass can borrow the buffers (Phase 6 —
    /// [`Application::display_list_text`]).
    pub dl_text_items: Vec<TextItem>,
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

    pub fn resize(&mut self, w: f32, h: f32) {
        let (w, h) = self.inner.as_ref().unwrap().adjust_size(w, h);
        if w > 0.0 && h > 0.0 {
            self.logical_width = w;
            self.logical_height = h;
            if let Some(ref mut renderer) = self.renderer {
                // wl_surface requires buffer dimensions divisible by the buffer
                // scale; snap up so a fractional logical size can't queue an
                // illegal swapchain extent. In forced-scale mode the surface
                // stays at buffer_scale 1 (the compositor believes scale 1).
                let s = if crate::scale::forced_scale().is_some() {
                    1
                } else {
                    (self.scale_factor.round() as u32).max(1)
                };
                let pw = ((w as f64 * self.scale_factor).round() as u32).max(1).div_ceil(s) * s;
                let ph = ((h as f64 * self.scale_factor).round() as u32).max(1).div_ceil(s) * s;
                renderer.resize(pw, ph);
            }
            let scale = self.scale_factor;
            self.inner.as_mut().unwrap().handle_resize(w, h, scale);
        }
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
        let logical_w = self.logical_width;
        let logical_h = self.logical_height;
        let scale_factor = self.scale_factor;

        if let Some(ref surface) = self.surface {
            if let Some(regions) = self.inner.as_ref().unwrap().input_regions() {
                let compositor = self.compositor_state.wl_compositor();
                let wl_region = compositor.create_region(&self.qh, ());
                for &(rx, ry, rw, rh) in &regions {
                    wl_region.add(rx, ry, rw, rh);
                }
                surface.set_input_region(Some(&wl_region));
                wl_region.destroy();
            }
        }
        
        // 1. The frame's geometry IS the app's display list — the single paint path. Tessellated
        // below as one batched, GPU-scissor-clipped pass. An app that draws nothing returns
        // `None`, giving an empty frame (the legacy view*/tuple-wrapping path is gone).
        let dl = self.inner.as_mut().unwrap()
            .display_list(LogicalSize::new(logical_w, logical_h), scale_factor)
            .unwrap_or_else(|| crate::scene::paint::PaintCtx::new().finish());

        // 1a. Phase 6 display-list text: shape the list's Text prims through the shared buffer
        // cache and hold them for the glyphon pass (the TextAreas built below borrow these).
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
                        color: glyphon::Color::rgba(
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

        if let Some(ref surface) = self.surface {
            let _callback = surface.frame(&self.qh, ());
            self.frame_callback_pending = true;
        }

        // Commit the buffer scale together with a buffer it is legal for: the
        // present inside draw_frame_2d is the only commit on this surface, so
        // sending the request here orders it right before a matching-size
        // attach+commit. Skipped while the pending extent isn't divisible (a
        // transition frame) — the old committed scale stays legal for it.
        if let Some(ref surface) = self.surface {
            let s = if crate::scale::forced_scale().is_some() {
                1
            } else {
                (self.scale_factor.round() as i32).max(1)
            };
            let e = renderer.pending_extent();
            if s != self.committed_buffer_scale
                && e.width % s as u32 == 0
                && e.height % s as u32 == 0
            {
                surface.set_buffer_scale(s);
                self.committed_buffer_scale = s;
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

        renderer.draw_frame_2d(Frame2D {
            verts: &verts,
            batches: &batches,
            overlay_verts: &overlay_verts,
            images: &image_quads,
            plate_features: &plate_features,
            clear_color,
        });
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
        _output: &wl_output::WlOutput,
    ) {
        self.redraw = true;
    }
    
    fn surface_leave(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        _output: &wl_output::WlOutput,
    ) {}
}

impl<A: Application> OutputHandler for EngineState<A> {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.output_state
    }
    
    fn new_output(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, _output: wl_output::WlOutput) {
        let scale = crate::wayland::detect_scale_factor(&self.output_state);
        crate::scale::set_scale_factor(scale as f32);
    }
    fn update_output(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, _output: wl_output::WlOutput) {
        let scale = crate::wayland::detect_scale_factor(&self.output_state);
        crate::scale::set_scale_factor(scale as f32);
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
        if let (Some(w), Some(h)) = (w, h) {
            let width = w.get();
            let height = h.get();
            // Forced mode: the compositor's logical size is really physical
            // pixels (scale-1 output); divide to get the app's logical space.
            let f = crate::scale::forced_scale().unwrap_or(1.0);
            self.resize(width as f32 / f, height as f32 / f);
        } else {
            let settings = self.inner.as_ref().unwrap().settings();
            let w = settings.width as f32;
            let h = settings.height as f32;
            self.resize(w, h);
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
    
    fn new_seat(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, seat: wl_seat::WlSeat) {
        self.seats.push(seat);
    }
    
    fn new_capability(
        &mut self,
        _conn: &Connection,
        qh: &QueueHandle<Self>,
        seat: wl_seat::WlSeat,
        capability: Capability,
    ) {
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
        let (mut last_lx, mut last_ly) = (0.0f32, 0.0f32);

        // Forced mode: pointer positions arrive in the compositor's scale-1
        // logical space (= physical); divide into the app's logical space.
        let forced = crate::scale::forced_scale().unwrap_or(1.0);
        for event in events {
            let (x, y) = event.position;
            let lx = x as f32 / forced;
            let ly = y as f32 / forced;

            self.cursor_pos = (lx, ly);
            match &event.kind {
                PointerEventKind::Enter { .. } => {
                    // Enter carries the pointer's position but no Motion follows until it
                    // actually moves — without this the app's hover state is stale from
                    // enter to first move, and a press in that window can misroute (e.g. a
                    // divider press falling through to the movable-backplate window drag).
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
                        } else if self.inner.as_ref().unwrap().is_movable_backplate_at(lx, ly) {
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
                PointerEventKind::Axis { horizontal, vertical, .. } => {
                    coalesced_h += horizontal.absolute;
                    coalesced_v += vertical.absolute;
                    discrete_h += horizontal.discrete;
                    discrete_v += vertical.discrete;
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
    ) {}
    
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

        let mut rebuild = false;
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

pub fn run<A: Application>() {
    let conn = Connection::connect_to_env().unwrap();
    let (globals, mut event_queue) = registry_queue_init(&conn).unwrap();
    let qh = event_queue.handle();

    let compositor_state = CompositorState::bind(&globals, &qh).unwrap();
    let xdg_shell_state = XdgShell::bind(&globals, &qh).unwrap();
    let layer_shell_state = LayerShell::bind(&globals, &qh).ok();
    let shm_state = Shm::bind(&globals, &qh).unwrap();
    let seat_state = SeatState::new(&globals, &qh);
    let output_state = OutputState::new(&globals, &qh);

    let (sender, channel) = calloop::channel::channel::<A::Message>();
    let pointer_gestures: Option<ZwpPointerGesturesV1> = globals.bind(&qh, 1..=3, ()).ok();

    let mut engine_state = EngineState {
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
        swash_cache: glyphon::SwashCache::new(),
        scale_factor: 1.0,
        committed_buffer_scale: 1,
        logical_width: 0.0,
        logical_height: 0.0,
        exit: false,
        redraw: false,
        frame_callback_pending: false,
        first_configure_received: false,
        ctrl_pressed: false,
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
        last_pinch_scale: 1.0,
        cursor_pos: (0.0, 0.0),
        last_press_serial: None,
        dl_text_items: Vec::new(),
    };

    event_queue.roundtrip(&mut engine_state).unwrap();

    let scale = detect_scale_factor(&engine_state.output_state);
    engine_state.scale_factor = scale;
    crate::scale::set_scale_factor(scale as f32);

    let inner = A::new(&qh, engine_state.sender.clone());
    let settings = inner.settings();
    crate::scale::set_app_id(settings.app_id.clone());
    engine_state.logical_width = settings.width as f32;
    engine_state.logical_height = settings.height as f32;
    engine_state.inner = Some(inner);

    let surface = engine_state.compositor_state.create_surface(&qh);
    // Forced-scale mode renders scaled-up into a buffer_scale-1 surface.
    let buffer_scale = if crate::scale::forced_scale().is_some() { 1 } else { scale as i32 };
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
        window.commit();
        engine_state.window = Some(window);
    }
    engine_state.surface = Some(surface);

    engine_state.init_gpu(&conn, settings.width as f32, settings.height as f32);
    engine_state
        .inner
        .as_mut()
        .unwrap()
        .renderer_init(engine_state.renderer.as_mut().unwrap());

    let mut event_loop = EventLoop::try_new().unwrap();
    let loop_handle = event_loop.handle();
    WaylandSource::new(conn.clone(), event_queue).insert(loop_handle.clone()).unwrap();

    loop_handle.insert_source(channel, |event, _metadata, app_state: &mut EngineState<A>| {
        if let calloop::channel::Event::Msg(msg) = event {
            let mut rebuild = false;
            app_state.inner.as_mut().unwrap().update(msg, &mut rebuild, &mut app_state.exit);
            if rebuild {
                app_state.redraw = true;
            }
        }
    }).unwrap();

    engine_state.inner.as_mut().unwrap().register_sources(&loop_handle);

    const KEY_REPEAT_DELAY: std::time::Duration = std::time::Duration::from_millis(500);
    const KEY_REPEAT_INTERVAL: std::time::Duration = std::time::Duration::from_millis(50);

    let mut last_title = settings.title.clone();
    let mut last_tick = std::time::Instant::now();
    loop {
        if let Err(e) = event_loop.dispatch(std::time::Duration::from_millis(16), &mut engine_state) {
            log::error!("[window_runner] Event loop error: {:?}", e);
            break;
        }
        // A protocol error kills the connection permanently, but it surfaces
        // through queue flushes whose errors calloop's WaylandSource swallows
        // (it only treats Io errors as fatal) — without this check the loop
        // spins forever on a dead display while wayland-backend re-prints the
        // error on every flush attempt.
        if let Some(perr) = conn.protocol_error() {
            log::error!("[window_runner] Fatal Wayland protocol error, exiting: {perr}");
            break;
        }
        if engine_state.exit {
            break;
        }

        let now = std::time::Instant::now();
        let mut dt = now.duration_since(last_tick).as_secs_f32();
        last_tick = now;
        if dt > 0.1 {
            dt = 0.1;
        }

        let mut rebuild = false;
        engine_state.inner.as_mut().unwrap().tick(dt, &mut rebuild);
        if rebuild {
            engine_state.redraw = true;
        }

        let just_configured = engine_state.just_configured;
        engine_state.just_configured = false;

        if !just_configured {
            if let Some((w, h)) = engine_state.inner.as_ref().unwrap().desired_size() {
                if (engine_state.logical_width - w as f32).abs() > 0.001 || (engine_state.logical_height - h as f32).abs() > 0.001 {
                    engine_state.resize(w as f32, h as f32);
                    engine_state.redraw = true;
                }
            }
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
                    };

                    if let Some(ctx) = engine_state.inner.as_mut().unwrap().ui_context_mut() {
                        ctx.ctrl_pressed = engine_state.ctrl_pressed;
                        ctx.shift_pressed = engine_state.shift_pressed;
                        ctx.alt_pressed = engine_state.alt_pressed;
                        ctx.logo_pressed = engine_state.logo_pressed;
                    }

                    let mut key_rebuild = false;
                    if let Some(msg) = engine_state.inner.as_mut().unwrap().handle_key_input(&custom_event, &mut key_rebuild) {
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

        if engine_state.redraw && !engine_state.frame_callback_pending {
            engine_state.redraw = false;
            if engine_state.first_configure_received {
                engine_state.render();
            }
        }
    }
    engine_state.inner.as_mut().unwrap().on_exit();
    crate::process::cleanup_spawned_processes();
}
