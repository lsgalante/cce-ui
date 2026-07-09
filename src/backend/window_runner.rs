use std::time::Instant;
use smithay_client_toolkit::{
    compositor::{CompositorHandler, CompositorState},
    delegate_compositor, delegate_keyboard, delegate_pointer, delegate_registry,
    delegate_seat, delegate_shm, delegate_xdg_shell, delegate_xdg_window, delegate_output, delegate_xdg_popup,
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
use smithay_client_toolkit::shell::xdg::popup::{Popup, PopupHandler, PopupConfigure};
use smithay_client_toolkit::shell::xdg::{XdgPositioner, XdgSurface};
use smithay_client_toolkit::reexports::protocols::xdg::shell::client::xdg_positioner::{Anchor, Gravity, ConstraintAdjustment};

use calloop::EventLoop;
use calloop_wayland_source::WaylandSource;
use glyphon::{
    Cache, FontSystem, Resolution, TextArea,
    TextBounds, Viewport, Buffer, Attrs, Metrics,
};
use crate::widget::{Element, TextItem, MouseButton, ElementState, MouseScrollDelta, KeyEvent, Key, NamedKey, Position};
use crate::wayland::{WaylandSurfaceHandle, detect_scale_factor};
use crate::backend::WgpuAdapter;

pub struct ActivePopup {
    pub sctk_popup: Popup,
    pub wgpu_surface: wgpu::Surface<'static>,
    pub config: wgpu::SurfaceConfiguration,
    pub logical_width: f32,
    pub logical_height: f32,
    pub configured: bool,
    pub viewport: Viewport,
    pub x: f32,
    pub y: f32,
    pub vertex_buffer: Option<wgpu::Buffer>,
}

#[derive(Hash, PartialEq, Eq, Clone)]
struct BufferCacheKey {
    text: String,
    size_milli: u32,
    font: Option<String>,
    is_vertical: bool,
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

fn make_text_buffer_with_font(fs: &mut FontSystem, text: &str, size: f32, font: Option<&str>) -> Buffer {
    get_text_buffer(fs, text, size, font)
}

#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Vertex {
    pub position: [f32; 2],
    pub color: [f32; 4],
    pub clip_circle: [f32; 3], // [cx, cy, r]
}

impl Vertex {
    const ATTRIBS: [wgpu::VertexAttribute; 3] = wgpu::vertex_attr_array![
        0 => Float32x2,
        1 => Float32x4,
        2 => Float32x3,
    ];

    pub fn desc() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Vertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &Self::ATTRIBS,
        }
    }
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

pub fn push_rounded_rect_vertices_corners(
    x: f32, y: f32, ww: f32, h: f32,
    radii: crate::widget::CornerRadii,
    sw: f32, sh: f32,
    color: [f32; 4],
    clip_circle: [f32; 3],
    clip_rect: Option<(f32, f32, f32, f32)>,
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
            
            let x0 = clamp_x(cx);
            let y0 = clamp_y(cy);
            let x1 = clamp_x(cx + r_tl * theta1.cos());
            let y1 = clamp_y(cy + r_tl * theta1.sin());
            let x2 = clamp_x(cx + r_tl * theta2.cos());
            let y2 = clamp_y(cy + r_tl * theta2.sin());
            
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
            
            let x0 = clamp_x(cx);
            let y0 = clamp_y(cy);
            let x1 = clamp_x(cx + r_tr * theta1.cos());
            let y1 = clamp_y(cy + r_tr * theta1.sin());
            let x2 = clamp_x(cx + r_tr * theta2.cos());
            let y2 = clamp_y(cy + r_tr * theta2.sin());
            
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
            
            let x0 = clamp_x(cx);
            let y0 = clamp_y(cy);
            let x1 = clamp_x(cx + r_br * theta1.cos());
            let y1 = clamp_y(cy + r_br * theta1.sin());
            let x2 = clamp_x(cx + r_br * theta2.cos());
            let y2 = clamp_y(cy + r_br * theta2.sin());
            
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
            
            let x0 = clamp_x(cx);
            let y0 = clamp_y(cy);
            let x1 = clamp_x(cx + r_bl * theta1.cos());
            let y1 = clamp_y(cy + r_bl * theta1.sin());
            let x2 = clamp_x(cx + r_bl * theta2.cos());
            let y2 = clamp_y(cy + r_bl * theta2.sin());
            
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
    let r = r.min(ww * 0.5).min(h * 0.5);

    let rad = crate::layout::light_source_position();
    let lx = rad.cos();
    let ly = -rad.sin();

    let edge_color = |factor: f32| -> [f32; 4] {
        let max_offset = crate::layout::bevel_depth();
        let offset = factor * max_offset;
        [
            (base_color[0] + offset).clamp(0.0, 1.0),
            (base_color[1] + offset).clamp(0.0, 1.0),
            (base_color[2] + offset).clamp(0.0, 1.0),
            base_color[3],
        ]
    };

    let top_color = edge_color(-ly);
    let left_color = edge_color(-lx);
    let bottom_color = edge_color(ly);
    let right_color = edge_color(lx);

    out.extend_from_slice(&quad_vertices_with_clip(x + r, y, ww - 2.0 * r, t, sw, sh, top_color, clip_circle));
    out.extend_from_slice(&quad_vertices_with_clip(x, y + r, t, h - 2.0 * r, sw, sh, left_color, clip_circle));
    out.extend_from_slice(&quad_vertices_with_clip(x + r, y + h - t, ww - 2.0 * r, t, sw, sh, bottom_color, clip_circle));
    out.extend_from_slice(&quad_vertices_with_clip(x + ww - t, y + r, t, h - 2.0 * r, sw, sh, right_color, clip_circle));

    let segments = 16;
    let corners = [
        (x + r, y + r, std::f32::consts::PI, 1.5 * std::f32::consts::PI), // Top-Left
        (x + ww - r, y + r, 1.5 * std::f32::consts::PI, 2.0 * std::f32::consts::PI), // Top-Right
        (x + ww - r, y + h - r, 0.0, 0.5 * std::f32::consts::PI), // Bottom-Right
        (x + r, y + h - r, 0.5 * std::f32::consts::PI, std::f32::consts::PI), // Bottom-Left
    ];

    for &(cx, cy, start_angle, end_angle) in &corners {
        for j in 0..segments {
            let theta1 = start_angle + (j as f32) * (end_angle - start_angle) / (segments as f32);
            let theta2 = start_angle + ((j + 1) as f32) * (end_angle - start_angle) / (segments as f32);
            let theta_mid = 0.5 * (theta1 + theta2);

            let factor = (theta_mid.cos() * lx + theta_mid.sin() * ly).clamp(-1.0, 1.0);
            let segment_color = edge_color(factor);

            push_arc_background_vertices(
                cx, cy, r, t,
                theta1, theta2,
                sw, sh, segment_color, 1, clip_circle,
                out,
            );
        }
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

    if r_tl > 0.1 {
        push_arc_background_vertices(
            x + r_tl, y + r_tl, r_tl, t,
            std::f32::consts::PI, 1.5 * std::f32::consts::PI,
            sw, sh, color, segments, clip_circle,
            out,
        );
    }
    if r_tr > 0.1 {
        push_arc_background_vertices(
            x + ww - r_tr, y + r_tr, r_tr, t,
            1.5 * std::f32::consts::PI, 2.0 * std::f32::consts::PI,
            sw, sh, color, segments, clip_circle,
            out,
        );
    }
    if r_br > 0.1 {
        push_arc_background_vertices(
            x + ww - r_br, y + h - r_br, r_br, t,
            0.0, 0.5 * std::f32::consts::PI,
            sw, sh, color, segments, clip_circle,
            out,
        );
    }
    if r_bl > 0.1 {
        push_arc_background_vertices(
            x + r_bl, y + h - r_bl, r_bl, t,
            0.5 * std::f32::consts::PI, std::f32::consts::PI,
            sw, sh, color, segments, clip_circle,
            out,
        );
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

pub fn widget_vertices(w: &dyn crate::widget::Element, sw: f32, sh: f32, clip_circle: [f32; 3]) -> Vec<Vertex> {
    let mut verts = Vec::new();
    push_widget_vertices(w, sw, sh, clip_circle, &mut verts);
    verts
}

pub fn push_widget_vertices(w: &dyn crate::widget::Element, sw: f32, sh: f32, clip_circle: [f32; 3], out: &mut Vec<Vertex>) {
    let (x, y, ww, h) = w.rect();
    let radii = w.corner_radii();
    if let Some(thickness) = w.plate_bevel() {
        let t = thickness;
        let inner_radii = crate::widget::CornerRadii {
            top_left: (radii.top_left - t).max(0.0),
            top_right: (radii.top_right - t).max(0.0),
            bottom_right: (radii.bottom_right - t).max(0.0),
            bottom_left: (radii.bottom_left - t).max(0.0),
        };
        push_rounded_rect_vertices_corners(x + t, y + t, ww - 2.0 * t, h - 2.0 * t, inner_radii, sw, sh, w.color(), clip_circle, None, out);
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

/// A contiguous run of vertices sharing one scissor rect (Phase 3 single paint path). `scissor` is
/// a logical-pixel clip (`None` = unclipped); `start..end` indexes the flat vertex buffer.
pub struct DlBatch {
    pub scissor: Option<crate::scene::layout::Rect>,
    pub start: u32,
    pub end: u32,
}

/// Tessellate a `scene::paint::DisplayList`'s geometry into a flat vertex buffer plus per-clip draw
/// batches, reusing the same tessellators as the legacy path so vertices are identical. `Text`
/// prims are skipped here — text is still rendered via the app's `text_areas()` path. `sw`/`sh` are
/// logical surface dimensions (as everywhere else). Consecutive prims sharing a clip are merged
/// into one batch.
pub fn tessellate_display_list(
    dl: &crate::scene::paint::DisplayList,
    sw: f32,
    sh: f32,
) -> (Vec<Vertex>, Vec<DlBatch>) {
    use crate::scene::paint::{Cap, Prim};
    let no = [0.0f32, 0.0, 0.0];
    let mut verts: Vec<Vertex> = Vec::new();
    let mut batches: Vec<DlBatch> = Vec::new();

    for item in &dl.items {
        let start = verts.len() as u32;
        match &item.prim {
            Prim::Text { .. } => continue, // text goes through the glyphon/text_areas path
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
            Prim::Bevel { rect, radii, color, depth } => {
                // Mirror push_widget_vertices' bevel branch: inset rounded fill + bevel edges.
                let t = *depth;
                let inner = crate::widget::CornerRadii::new(
                    (radii.0 - t).max(0.0),
                    (radii.1 - t).max(0.0),
                    (radii.2 - t).max(0.0),
                    (radii.3 - t).max(0.0),
                );
                push_rounded_rect_vertices_corners(rect.x + t, rect.y + t, rect.width - 2.0 * t, rect.height - 2.0 * t, inner, sw, sh, *color, no, None, &mut verts);
                push_plate_bevel_vertices(rect.x, rect.y, rect.width, rect.height, radii.0, t, sw, sh, *color, no, &mut verts);
            }
            Prim::Arc { cx, cy, radius, thickness, start: sa, end: ea, color } => {
                push_arc_background_vertices(*cx, *cy, *radius, *thickness, *sa, *ea, sw, sh, *color, 16, no, &mut verts);
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
                verts.extend(circle_vertices(*cx, *cy, *radius, sw, sh, *color, 16, no));
            }
        }
        let end = verts.len() as u32;
        if end == start {
            continue;
        }
        // Merge into the previous batch if it shares this clip and is contiguous.
        if let Some(last) = batches.last_mut() {
            if last.scissor == item.clip && last.end == start {
                last.end = end;
                continue;
            }
        }
        batches.push(DlBatch { scissor: item.clip, start, end });
    }

    (verts, batches)
}

pub fn extra_quad_vertices(
    w: &dyn crate::widget::Element,
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
    w: &'a dyn crate::widget::Element,
    qx: f32, qy: f32, qw: f32, qh: f32,
) -> &'a dyn crate::widget::Element {
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
        for cb_opt in &pbg.checkboxes {
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
        for &child_ptr in &pbg.children {
            let child = unsafe { &*child_ptr };
            let (cx, cy, cww, chh) = child.rect();
            if qx >= cx - 0.1 && qx + qw <= cx + cww + 0.1 && qy >= cy - 0.1 && qy + qh <= cy + chh + 0.1 {
                return child;
            }
        }
    }
    if let Some(mc) = w.as_any().downcast_ref::<crate::widget::input::MultiControl>() {
        let (bx, by, bw, bh) = mc.add_button.rect();
        if qx >= bx - 0.1 && qx + qw <= bx + bw + 0.1 && qy >= by - 0.1 && qy + qh <= by + bh + 0.1 {
            return &mc.add_button;
        }
        for row in &mc.rows {
            let (kx, ky, kw, kh) = row.key_input.rect();
            if qx >= kx - 0.1 && qx + qw <= kx + kw + 0.1 && qy >= ky - 0.1 && qy + qh <= ky + kh + 0.1 {
                return &row.key_input;
            }
            let (tx, ty, tw, th) = row.type_dropdown.rect();
            if qx >= tx - 0.1 && qx + qw <= tx + tw + 0.1 && qy >= ty - 0.1 && qy + qh <= ty + th + 0.1 {
                return &row.type_dropdown;
            }
            let (rx, ry, rw, rh) = row.remove_button.rect();
            if qx >= rx - 0.1 && qx + qw <= rx + rw + 0.1 && qy >= ry - 0.1 && qy + qh <= ry + rh + 0.1 {
                return &row.remove_button;
            }
            let (vx, vy, vw, vh) = row.value_widget.rect();
            if qx >= vx - 0.1 && qx + qw <= vx + vw + 0.1 && qy >= vy - 0.1 && qy + qh <= vy + vh + 0.1 {
                return match &row.value_widget {
                    crate::widget::input::InstancedWidget::TextBox(w) => w,
                    crate::widget::input::InstancedWidget::Spinbox(w) => w,
                    crate::widget::input::InstancedWidget::Toggle(w) => w,
                    crate::widget::input::InstancedWidget::Slider(w) => w,
                };
            }
        }
    }
    if let Some(kc) = w.as_any().downcast_ref::<crate::widget::input::KeybindsControl>() {
        let (bx, by, bw, bh) = kc.add_button.rect();
        if qx >= bx - 0.1 && qx + qw <= bx + bw + 0.1 && qy >= by - 0.1 && qy + qh <= by + bh + 0.1 {
            return &kc.add_button;
        }
        for row in &kc.rows {
            let (kx, ky, kw, kh) = row.key_input.rect();
            if qx >= kx - 0.1 && qx + qw <= kx + kw + 0.1 && qy >= ky - 0.1 && qy + qh <= ky + kh + 0.1 {
                return &row.key_input;
            }
            let (cx, cy, cw, ch) = row.cmd_input.rect();
            if qx >= cx - 0.1 && qx + qw <= cx + cw + 0.1 && qy >= cy - 0.1 && qy + qh <= cy + ch + 0.1 {
                return &row.cmd_input;
            }
            let (rx, ry, rw, rh) = row.remove_button.rect();
            if qx >= rx - 0.1 && qx + qw <= rx + rw + 0.1 && qy >= ry - 0.1 && qy + qh <= ry + rh + 0.1 {
                return &row.remove_button;
            }
        }
    }
    w
}

pub fn push_extra_quad_vertices(
    w: &dyn crate::widget::Element,
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
    w: &dyn crate::widget::Element,
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
    w: &dyn crate::widget::Element,
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
    /// Legacy geometry sink. Default no-op since Phase 6: an app whose whole frame comes from
    /// [`display_list`](Application::display_list) (+ [`display_list_text`]) implements neither
    /// this nor [`text_items`](Application::text_items).
    fn view(&mut self, _quads: &mut Vec<(f32, f32, f32, f32, [f32; 4])>, _size: LogicalSize, _scale: f64) {}
    fn view_rounded_quads(&mut self, _quads: &mut Vec<(f32, f32, f32, f32, f32, [f32; 4], (bool, bool, bool, bool))>, _size: LogicalSize, _scale: f64) {}
    fn view_vectors(&mut self, _vectors: &mut Vec<(f32, f32, f32, f32, f32, [f32; 4], LineCap)>, _size: LogicalSize, _scale: f64) {}
    fn overlay_quads(&mut self, _quads: &mut Vec<(f32, f32, f32, f32, [f32; 4])>, _size: LogicalSize, _scale: f64) {}
    fn text_items(&self) -> &[TextItem] {
        &[]
    }
    fn render_popovers(&self, _pc: &mut dyn crate::layout::RenderTarget) {}
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

    fn is_movable_backplate_at(&self, px: f32, py: f32) -> bool {
        if let Some(ctx) = self.ui_context() {
            ctx.is_movable_backplate_at(px, py)
        } else {
            false
        }
    }
    
    fn text_areas(&self, scale_f32: f32, bounds: TextBounds) -> Vec<TextArea<'_>> {
        let mut overlay_rects: Vec<(f32, f32, f32, f32)> = Vec::new();
        if let Some(ctx) = self.ui_context() {
            for popover_ptr in &ctx.active_popovers {
                unsafe {
                    if let Some(popover) = popover_ptr.as_ref() {
                        if let Some((x, y, w, h)) = popover.popover_rect() {
                            overlay_rects.push((x, y, w, h));
                        }
                    }
                }
            }
        }

        self.text_items().iter().map(|ti| {
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

            for &(ox, oy, ow, oh) in &overlay_rects {
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

            TextArea {
                buffer: &ti.buffer,
                left: (ti.x * scale_f32).round(),
                top: (ti.y * scale_f32).round(),
                scale: 1.0,
                bounds: item_bounds,
                default_color: ti.color,
                custom_glyphs: &[],
            }
        }).collect()
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

    /// Opt into the single paint path (Phase 3): return a display list for this frame and `render()`
    /// draws its geometry via one batched, GPU-scissor-clipped pass instead of the legacy
    /// `view*` geometry. Default `None` keeps the legacy path. Text, overlays, and `custom_vertices`
    /// still go through their existing paths. Typically implemented as
    /// `Some(cce_ui::scene::painter::paint_tree(&self.ui_context, root_ptr))`.
    fn display_list(&mut self) -> Option<crate::scene::paint::DisplayList> {
        None
    }

    /// Phase 6 opt-in: render the display list's `Prim::Text` items through the glyphon pass
    /// (shaped via the shared buffer cache, clipped to the item clip ∩ the prim bounds). A
    /// fully migrated app's ENTIRE frame — geometry and text — is then one
    /// [`display_list`](Application::display_list); its [`text_items`](Application::text_items)
    /// is typically empty. Default `false`: the seven Phase 3 adopters' lists already carry
    /// Text prims that those apps ALSO push as `TextItem`s — rendering both would double-draw,
    /// so each app flips this only when it stops pushing its own.
    ///
    /// Known limitation (matching scope, not a bug): the legacy `text_areas` popover-occlusion
    /// clipping is not applied to display-list text — text renders after all geometry, so a
    /// popover's plate does not hide list text beneath it yet.
    fn display_list_text(&self) -> bool {
        false
    }
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
    
    pub wgpu_adapter: Option<WgpuAdapter>,
    pub render_pipeline: Option<wgpu::RenderPipeline>,
    pub vertex_buffer: Option<wgpu::Buffer>,
    pub vertex_count: u32,
    pub overlay_vertex_buffer: Option<wgpu::Buffer>,
    pub overlay_vertex_count: u32,
    
    pub scale_factor: f64,
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
    pub active_popup: Option<ActivePopup>,
    pub current_cursor_icon: Option<CursorIcon>,
    pub qh: QueueHandle<EngineState<A>>,
    pub just_configured: bool,
    pub pointer_gestures: Option<ZwpPointerGesturesV1>,
    pub pinch_gesture: Option<ZwpPointerGesturePinchV1>,
    pub last_pinch_scale: f32,
    pub cursor_pos: (f32, f32),
    /// This frame's display-list text, shaped and held here so the glyphon `TextArea`s built
    /// in the render pass can borrow the buffers (Phase 6 —
    /// [`Application::display_list_text`]).
    pub dl_text_items: Vec<TextItem>,
}

impl<A: Application> EngineState<A> {
    pub async fn init_gpu(&mut self, conn: &Connection, width_logical: f32, height_logical: f32) {
        let s = self.scale_factor as f32;
        let pw = (width_logical * s) as u32;
        let ph = (height_logical * s) as u32;
        
        let surface = self.surface.as_ref().expect("surface missing");
        
        let display_ptr = conn.backend().display_id().as_ptr() as *mut std::ffi::c_void;
        let surface_ptr = surface.id().as_ptr() as *mut std::ffi::c_void;
        
        let adapter = WgpuAdapter::new(display_ptr, surface_ptr, pw, ph).await;
        
        let shader = adapter.device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Shader"),
            source: wgpu::ShaderSource::Wgsl(crate::SHADER.into()),
        });
        
        let pipeline_layout = adapter.device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Pipeline Layout"),
            bind_group_layouts: &[],
            push_constant_ranges: &[],
        });
        
        let render_pipeline = adapter.device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Render Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[Vertex::desc()],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: adapter.config.format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
                strip_index_format: None,
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState { count: 1, mask: !0, alpha_to_coverage_enabled: false },
            multiview: None,
            cache: None,
        });
        
        let vertex_buffer = adapter.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Vertex Buffer"),
            size: 1,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let overlay_vertex_buffer = adapter.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Overlay Vertex Buffer"),
            size: 1,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        
        self.wgpu_adapter = Some(adapter);
        self.render_pipeline = Some(render_pipeline);
        self.vertex_buffer = Some(vertex_buffer);
        self.overlay_vertex_buffer = Some(overlay_vertex_buffer);
        self.logical_width = width_logical;
        self.logical_height = height_logical;
    }
    
    pub fn resize(&mut self, w: f32, h: f32) {
        let (w, h) = self.inner.as_ref().unwrap().adjust_size(w, h);
        if w > 0.0 && h > 0.0 {
            self.logical_width = w;
            self.logical_height = h;
            if let Some(ref mut adapter) = self.wgpu_adapter {
                adapter.resize((w as f64 * self.scale_factor) as u32, (h as f64 * self.scale_factor) as u32);
            }
        }
    }
    
    pub fn render(&mut self) {
        let logical_w = self.logical_width;
        let logical_h = self.logical_height;
        let scale_factor = self.scale_factor;
        
        let mut quads = Vec::new();
        self.inner.as_mut().unwrap().view(&mut quads, LogicalSize::new(logical_w, logical_h), scale_factor);

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
        
        let mut rounded_quads = Vec::new();
        self.inner.as_mut().unwrap().view_rounded_quads(&mut rounded_quads, LogicalSize::new(logical_w, logical_h), scale_factor);
        
        let mut vectors = Vec::new();
        self.inner.as_mut().unwrap().view_vectors(&mut vectors, LogicalSize::new(logical_w, logical_h), scale_factor);

        // Phase 3 single paint path: when the app provides a display list, its geometry replaces the
        // legacy view* geometry and is drawn as batched, GPU-scissor-clipped runs. Default `None`
        // keeps the legacy path byte-for-byte.
        let display_list = self.inner.as_mut().unwrap().display_list();

        // 1. Build the frame's DisplayList — from the app's display_list() when provided, otherwise
        // by wrapping its legacy view*/view_vectors geometry — then tessellate it as one path. This
        // is the Phase 3 single paint path: every app, migrated or not, renders through here.
        let dl = match display_list {
            Some(dl) => dl,
            None => {
                use crate::scene::layout::Rect;
                use crate::scene::paint::{Cap, PaintCtx};
                let mut pc = PaintCtx::new();
                for &(qx, qy, qw, qh, qr, qc, qcorners) in &rounded_quads {
                    let rect = Rect { x: qx, y: qy, width: qw, height: qh };
                    if qr > 0.1 {
                        pc.rounded_rect(rect, qr, qcorners, qc);
                    } else {
                        pc.quad(rect, qc);
                    }
                }
                for &(qx, qy, qw, qh, qc) in &quads {
                    pc.quad(Rect { x: qx, y: qy, width: qw, height: qh }, qc);
                }
                for &(vx1, vy1, vx2, vy2, vthickness, vcolor, vcap) in &vectors {
                    let cap = match vcap {
                        LineCap::Flat => Cap::Flat,
                        LineCap::Round => Cap::Round,
                        LineCap::Arrow => Cap::Arrow,
                    };
                    pc.vector(vx1, vy1, vx2, vy2, vthickness, vcolor, cap);
                }
                pc.finish()
            }
        };

        // 1a. Phase 6 display-list text: shape the list's Text prims through the shared buffer
        // cache and hold them for the glyphon pass (the TextAreas built below borrow these).
        // Clip = the paint walk's item clip ∩ the prim's own bounds, in logical space.
        self.dl_text_items.clear();
        if self.inner.as_ref().unwrap().display_list_text() {
            let fs = &mut self.wgpu_adapter.as_mut().unwrap().font_system;
            for item in &dl.items {
                if let crate::scene::paint::Prim::Text { text, x, y, font_size, color, font, bounds } = &item.prim {
                    let clip = item.clip.map(|c| [c.x, c.y, c.x + c.width, c.y + c.height]);
                    let merged = match (clip, *bounds) {
                        (Some(a), Some(b)) => Some([a[0].max(b[0]), a[1].max(b[1]), a[2].min(b[2]), a[3].min(b[3])]),
                        (Some(a), None) => Some(a),
                        (None, b) => b,
                    };
                    let buffer = get_text_buffer(fs, text, *font_size, font.as_deref());
                    self.dl_text_items.push(TextItem {
                        buffer,
                        x: *x,
                        y: *y,
                        color: glyphon::Color::rgb(color[0], color[1], color[2]),
                        bounds: merged,
                    });
                }
            }
        }

        let adapter = self.wgpu_adapter.as_mut().unwrap();
        let render_pipeline = self.render_pipeline.as_ref().unwrap();
        let (mut verts, mut dl_batches) = tessellate_display_list(&dl, logical_w, logical_h);
        // custom_vertices (e.g. graph geometry) is appended as a final unclipped batch drawn on top.
        let pre_custom = verts.len() as u32;
        self.inner.as_mut().unwrap().custom_vertices(&mut verts, LogicalSize::new(logical_w, logical_h), scale_factor);
        if (verts.len() as u32) > pre_custom {
            dl_batches.push(DlBatch { scissor: None, start: pre_custom, end: verts.len() as u32 });
        }
        self.vertex_count = verts.len() as u32;
        if self.vertex_count > 0 {
            let data = bytemuck::cast_slice(&verts);
            let needed = data.len() as wgpu::BufferAddress;
            let mut vbuf = self.vertex_buffer.as_ref().unwrap();
            if needed > vbuf.size() {
                let new_vbuf = adapter.device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("Vertex Buffer"),
                    size: needed,
                    usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                });
                self.vertex_buffer = Some(new_vbuf);
                vbuf = self.vertex_buffer.as_ref().unwrap();
            }
            adapter.queue.write_buffer(vbuf, 0, data);
        }

        // 1b. Build and upload overlay vertex buffer
        let mut overlay_quads = Vec::new();
        self.inner.as_mut().unwrap().overlay_quads(&mut overlay_quads, LogicalSize::new(logical_w, logical_h), scale_factor);
        let mut overlay_verts = Vec::new();
        for &(qx, qy, qw, qh, qc) in &overlay_quads {
            overlay_verts.extend(quad_vertices(qx, qy, qw, qh, logical_w, logical_h, qc));
        }
        self.overlay_vertex_count = overlay_verts.len() as u32;
        if self.overlay_vertex_count > 0 {
            let data = bytemuck::cast_slice(&overlay_verts);
            let needed = data.len() as wgpu::BufferAddress;
            let mut ovbuf = self.overlay_vertex_buffer.as_ref().unwrap();
            if needed > ovbuf.size() {
                let new_ovbuf = adapter.device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("Overlay Vertex Buffer"),
                    size: needed,
                    usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                });
                self.overlay_vertex_buffer = Some(new_ovbuf);
                ovbuf = self.overlay_vertex_buffer.as_ref().unwrap();
            }
            adapter.queue.write_buffer(ovbuf, 0, data);
        }
        
        // 2. Prepare text
        let scale_f32 = scale_factor as f32;
        let pw = (logical_w * scale_f32) as u32;
        let ph = (logical_h * scale_f32) as u32;
        adapter.text_viewport.update(&adapter.queue, Resolution { width: pw, height: ph });
        
        let bounds = TextBounds { left: 0, top: 0, right: pw as i32, bottom: ph as i32 };
        let mut areas = self.inner.as_ref().unwrap().text_areas(scale_f32, bounds);
        // Phase 6 display-list text — the default `text_areas` mapping (scale + surface clamp),
        // without the popover-occlusion pass (see `Application::display_list_text`).
        for ti in &self.dl_text_items {
            let item_bounds = if let Some([l, t, r, b]) = ti.bounds {
                TextBounds {
                    left: ((l * scale_f32).round() as i32).clamp(0, bounds.right),
                    top: ((t * scale_f32).round() as i32).clamp(0, bounds.bottom),
                    right: ((r * scale_f32).round() as i32).clamp(0, bounds.right),
                    bottom: ((b * scale_f32).round() as i32).clamp(0, bounds.bottom),
                }
            } else {
                bounds
            };
            areas.push(TextArea {
                buffer: &ti.buffer,
                left: (ti.x * scale_f32).round(),
                top: (ti.y * scale_f32).round(),
                scale: 1.0,
                bounds: item_bounds,
                default_color: ti.color,
                custom_glyphs: &[],
            });
        }
        adapter.text_renderer.prepare(&adapter.device, &adapter.queue, &mut adapter.font_system, &mut adapter.text_atlas, &adapter.text_viewport, areas, &mut adapter.swash_cache).unwrap();
        
        // 3. Render Pass
        let output = match adapter.surface.get_current_texture() {
            Ok(t) => t,
            Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                adapter.surface.configure(&adapter.device, &adapter.config);
                match adapter.surface.get_current_texture() {
                    Ok(t) => t,
                    Err(e) => {
                        log::error!("Surface error after configure: {e:?}");
                        return;
                    }
                }
            }
            Err(wgpu::SurfaceError::Timeout) => return,
            Err(e) => {
                log::error!("Surface error: {e:?}");
                return;
            }
        };
        let view = output.texture.create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = adapter.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Encoder"),
        });
        
        {
            let cc = self.inner.as_ref().unwrap().clear_color();
            let r_clear = (cc[0] as f64).powf(2.2);
            let g_clear = (cc[1] as f64).powf(2.2);
            let b_clear = (cc[2] as f64).powf(2.2);
            let a_clear = cc[3] as f64;
            
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Render Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: r_clear,
                            g: g_clear,
                            b: b_clear,
                            a: a_clear,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            
            if self.vertex_count > 0 {
                pass.set_pipeline(render_pipeline);
                pass.set_vertex_buffer(0, self.vertex_buffer.as_ref().unwrap().slice(..));
                // Draw each clip batch under its own GPU scissor (logical clip -> physical px).
                for batch in &dl_batches {
                    match batch.scissor {
                        Some(clip) => {
                            let sx = (clip.x * scale_f32).max(0.0) as u32;
                            let sy = (clip.y * scale_f32).max(0.0) as u32;
                            if sx >= pw || sy >= ph {
                                continue;
                            }
                            let sw_px = ((clip.width * scale_f32) as u32).min(pw - sx);
                            let sh_px = ((clip.height * scale_f32) as u32).min(ph - sy);
                            if sw_px == 0 || sh_px == 0 {
                                continue;
                            }
                            pass.set_scissor_rect(sx, sy, sw_px, sh_px);
                        }
                        None => pass.set_scissor_rect(0, 0, pw, ph),
                    }
                    pass.draw(batch.start..batch.end, 0..1);
                }
                // Restore full scissor so text/overlay draws are not clipped.
                pass.set_scissor_rect(0, 0, pw, ph);
            }

            adapter.text_renderer.render(&adapter.text_atlas, &adapter.text_viewport, &mut pass).unwrap();
 
            if self.overlay_vertex_count > 0 {
                pass.set_pipeline(render_pipeline);
                pass.set_vertex_buffer(0, self.overlay_vertex_buffer.as_ref().unwrap().slice(..));
                pass.draw(0..self.overlay_vertex_count, 0..1);
            }
        }
        
        if let Some(ref surface) = self.surface {
            let _callback = surface.frame(&self.qh, ());
            self.frame_callback_pending = true;
        }

        adapter.queue.submit(std::iter::once(encoder.finish()));
        output.present();
        
        if let Some(ref mut popup) = self.active_popup {
            if popup.configured {
                let mut collector = crate::layout::PopoverCollector::new();
                self.inner.as_mut().unwrap().render_popovers(&mut collector);
 
                let mut verts = Vec::new();
                for &(color, qx, qy, qw, qh) in &collector.rects {
                    let qx_local = qx - popup.x;
                    let qy_local = qy - popup.y;
                    verts.extend(quad_vertices(qx_local, qy_local, qw, qh, popup.logical_width, popup.logical_height, color));
                }
 
                let vertex_count = verts.len() as u32;
                if vertex_count > 0 {
                    let data = bytemuck::cast_slice(&verts);
                    let needed = data.len() as wgpu::BufferAddress;
                    let mut vbuf = popup.vertex_buffer.as_ref();
                    if vbuf.map_or(true, |v| needed > v.size()) {
                        let new_vbuf = adapter.device.create_buffer(&wgpu::BufferDescriptor {
                            label: Some("Popup Vertex Buffer"),
                            size: needed,
                            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                            mapped_at_creation: false,
                        });
                        popup.vertex_buffer = Some(new_vbuf);
                        vbuf = popup.vertex_buffer.as_ref();
                    }
                    adapter.queue.write_buffer(vbuf.unwrap(), 0, data);
                }
 
                let mut text_items = Vec::new();
                for (content, size, tx, ty, color, font, bounds) in collector.texts {
                    let tx_local = tx - popup.x;
                    let ty_local = ty - popup.y;
                    text_items.push(TextItem {
                        buffer: make_text_buffer_with_font(&mut adapter.font_system, &content, size, font.as_deref()),
                        x: tx_local,
                        y: ty_local,
                        color: glyphon::Color::rgb(
                            (color[0] * 255.0).clamp(0.0, 255.0) as u8,
                            (color[1] * 255.0).clamp(0.0, 255.0) as u8,
                            (color[2] * 255.0).clamp(0.0, 255.0) as u8,
                        ),
                        bounds,
                    });
                }
 
                let scale_f32 = scale_factor as f32;
                let bounds = TextBounds {
                    left: 0,
                    top: 0,
                    right: (popup.logical_width * scale_f32) as i32,
                    bottom: (popup.logical_height * scale_f32) as i32,
                };
                let areas: Vec<TextArea<'_>> = text_items.iter().map(|ti| TextArea {
                    buffer: &ti.buffer,
                    left: (ti.x * scale_f32).round(),
                    top: (ti.y * scale_f32).round(),
                    scale: 1.0,
                    bounds,
                    default_color: ti.color,
                    custom_glyphs: &[],
                }).collect();
 
                popup.viewport.update(&adapter.queue, Resolution {
                    width: (popup.logical_width * scale_f32) as u32,
                    height: (popup.logical_height * scale_f32) as u32,
                });
 
                adapter.text_renderer.prepare(
                    &adapter.device,
                    &adapter.queue,
                    &mut adapter.font_system,
                    &mut adapter.text_atlas,
                    &popup.viewport,
                    areas,
                    &mut adapter.swash_cache,
                ).unwrap();
 
                let popup_output = match popup.wgpu_surface.get_current_texture() {
                    Ok(t) => t,
                    Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                        popup.wgpu_surface.configure(&adapter.device, &popup.config);
                        match popup.wgpu_surface.get_current_texture() {
                            Ok(t) => t,
                            Err(e) => {
                                log::error!("Popup surface error after configure: {e:?}");
                                return;
                            }
                        }
                    }
                    Err(wgpu::SurfaceError::Timeout) => return,
                    Err(e) => {
                        log::error!("Popup surface error: {e:?}");
                        return;
                    }
                };
                let popup_view = popup_output.texture.create_view(&wgpu::TextureViewDescriptor::default());
                let mut popup_encoder = adapter.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("Popup Encoder"),
                });
 
                {
                    let mut pass = popup_encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                        label: Some("Popup Render Pass"),
                        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                            view: &popup_view,
                            resolve_target: None,
                            ops: wgpu::Operations {
                                load: wgpu::LoadOp::Clear(wgpu::Color { r: 0.0, g: 0.0, b: 0.0, a: 0.0 }),
                                store: wgpu::StoreOp::Store,
                            },
                        })],
                        depth_stencil_attachment: None,
                        timestamp_writes: None,
                        occlusion_query_set: None,
                    });
 
                    if vertex_count > 0 {
                        pass.set_pipeline(render_pipeline);
                        pass.set_vertex_buffer(0, popup.vertex_buffer.as_ref().unwrap().slice(..));
                        pass.draw(0..vertex_count, 0..1);
                    }
 
                    adapter.text_renderer.render(&adapter.text_atlas, &popup.viewport, &mut pass).unwrap();
                }
 
                adapter.queue.submit(std::iter::once(popup_encoder.finish()));
                popup_output.present();
            }
        }
 
        adapter.text_atlas.trim();
    }
}

impl<A: Application> Drop for EngineState<A> {
    fn drop(&mut self) {
        self.wgpu_adapter = None;
        self.render_pipeline = None;
        self.vertex_buffer = None;
        self.overlay_vertex_buffer = None;
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
        _surface.set_buffer_scale(scale_factor);
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
        eprintln!("CONFIGURE_NEW_SIZE: w={:?}, h={:?}, scale={}", w, h, self.scale_factor);
        if let (Some(w), Some(h)) = (w, h) {
            let width = w.get();
            let height = h.get();
            self.resize(width as f32, height as f32);
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

        for event in events {
            let (x, y) = event.position;
            let mut lx = x as f32;
            let mut ly = y as f32;
            
            if let Some(ref popup) = self.active_popup {
                if event.surface == *popup.sctk_popup.wl_surface() {
                    lx += popup.x;
                    ly += popup.y;
                }
            }
            
            self.cursor_pos = (lx, ly);
            
            match &event.kind {
                PointerEventKind::Enter { .. } => {
                    let is_status_bar = self.inner.as_ref().unwrap().settings().app_id.starts_with("cce-status");
                    let mut cursor_icon = CursorIcon::Default;
                    if !is_status_bar {
                        let border = 8.0f32;
                        if ly < border {
                            if lx < border {
                                cursor_icon = CursorIcon::NwResize;
                            } else if lx > self.logical_width - border {
                                cursor_icon = CursorIcon::NeResize;
                            } else {
                                cursor_icon = CursorIcon::NResize;
                            }
                        } else if ly > self.logical_height - border {
                            if lx < border {
                                cursor_icon = CursorIcon::SwResize;
                            } else if lx > self.logical_width - border {
                                cursor_icon = CursorIcon::SeResize;
                            } else {
                                cursor_icon = CursorIcon::SResize;
                            }
                        } else if lx < border {
                            cursor_icon = CursorIcon::WResize;
                        } else if lx > self.logical_width - border {
                            cursor_icon = CursorIcon::EResize;
                        }
                    }
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

                    let is_status_bar = self.inner.as_ref().unwrap().settings().app_id.starts_with("cce-status");
                    let mut cursor_icon = CursorIcon::Default;
                    if !is_status_bar {
                        let border = 8.0f32;
                        if ly < border {
                            if lx < border {
                                cursor_icon = CursorIcon::NwResize;
                            } else if lx > self.logical_width - border {
                                cursor_icon = CursorIcon::NeResize;
                            } else {
                                cursor_icon = CursorIcon::NResize;
                            }
                        } else if ly > self.logical_height - border {
                            if lx < border {
                                cursor_icon = CursorIcon::SwResize;
                            } else if lx > self.logical_width - border {
                                cursor_icon = CursorIcon::SeResize;
                            } else {
                                cursor_icon = CursorIcon::SResize;
                            }
                        } else if lx < border {
                            cursor_icon = CursorIcon::WResize;
                        } else if lx > self.logical_width - border {
                            cursor_icon = CursorIcon::EResize;
                        }
                    }

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

                    // Client-Side Decorations (CSD) Drag & Resize Handling
                    let is_status_bar = self.inner.as_ref().unwrap().settings().app_id.starts_with("cce-status");
                    if btn == MouseButton::Left && !is_status_bar {
                        let border = 8.0f32;
                        let mut edge = smithay_client_toolkit::reexports::protocols::xdg::shell::client::xdg_toplevel::ResizeEdge::None;
                        if ly < border {
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
                        if !is_widget && ly >= border && ly < 32.0 && lx < self.logical_width - 70.0 {
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
            let delta = if discrete_h == 0 && discrete_v == 0 {
                // Pixel scroll event from touchpad / smooth mouse
                MouseScrollDelta::PixelDelta(Position {
                    x: -coalesced_h,
                    y: -coalesced_v,
                })
            } else {
                // Discrete scroll event (e.g. wheel clicks)
                let h_lines = if discrete_h != 0 { discrete_h as f32 } else { coalesced_h as f32 / 10.0 };
                let v_lines = if discrete_v != 0 { discrete_v as f32 } else { coalesced_v as f32 / 10.0 };
                MouseScrollDelta::LineDelta(-h_lines, -v_lines)
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
            xkeysym::Keysym::Tab => Key::Named(NamedKey::Tab),
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
                if let Some(ref text) = event.utf8 {
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

impl<A: Application> PopupHandler for EngineState<A> {
    fn configure(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _popup: &Popup,
        _config: PopupConfigure,
    ) {
        if let Some(ref mut p) = self.active_popup {
            p.configured = true;
        }
        self.redraw = true;
    }

    fn done(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, _popup: &Popup) {
        for popover_ptr in crate::widget::popovers::get_active() {
            unsafe {
                let popover = &mut *(popover_ptr as *mut dyn crate::widget::Element);
                popover.unfocus();
            }
        }
        crate::widget::context_menu::hide();
        self.active_popup = None;
        self.redraw = true;
    }
}

delegate_compositor!(@<A: Application> EngineState<A>);
delegate_xdg_shell!(@<A: Application> EngineState<A>);
delegate_xdg_window!(@<A: Application> EngineState<A>);
delegate_xdg_popup!(@<A: Application> EngineState<A>);
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
        wgpu_adapter: None,
        render_pipeline: None,
        vertex_buffer: None,
        vertex_count: 0,
        overlay_vertex_buffer: None,
        overlay_vertex_count: 0,
        scale_factor: 1.0,
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
        active_popup: None,
        current_cursor_icon: None,
        qh: qh.clone(),
        just_configured: false,
        pointer_gestures,
        pinch_gesture: None,
        last_pinch_scale: 1.0,
        cursor_pos: (0.0, 0.0),
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
    surface.set_buffer_scale(scale as i32);

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

    pollster::block_on(engine_state.init_gpu(&conn, settings.width as f32, settings.height as f32));

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

        let active_popovers = crate::widget::popovers::get_active();
        let context_menu_visible = crate::widget::context_menu::is_visible();

        let mut active_popover_rect = None;
        if !active_popovers.is_empty() {
            let popover_widget = unsafe { &*active_popovers[0] };
            active_popover_rect = popover_widget.popover_rect();
        }

        if active_popover_rect.is_some() || context_menu_visible {
            let (px, py, mut pw, mut ph, is_context_menu) = if let Some((x, y, w, h)) = active_popover_rect {
                (x, y, w.max(1.0), h.max(1.0), false)
            } else {
                (
                    crate::widget::context_menu::x(),
                    crate::widget::context_menu::y(),
                    crate::widget::context_menu::w().max(1.0),
                    crate::widget::context_menu::h().max(1.0),
                    true,
                )
            };
            pw = pw.ceil();
            ph = ph.ceil();
            if engine_state.active_popup.is_none() && engine_state.window.is_some() {
                let positioner = XdgPositioner::new(&engine_state.xdg_shell_state).unwrap();
                positioner.set_size(pw as i32, ph as i32);
                
                if !is_context_menu {
                    let popover_widget = unsafe { &*active_popovers[0] };
                    let (rx, ry, rw, rh) = popover_widget.rect();
                    let scroll_y = crate::widget::hover_animation::get_scroll_offset();
                    let screen_ry = ry - scroll_y;
                    let lw = engine_state.logical_width as i32;
                    let lh = engine_state.logical_height as i32;
                    let ax = (rx as i32).clamp(0, (lw - 1).max(0));
                    let ay = (screen_ry as i32).clamp(0, (lh - 1).max(0));
                    let aw = (rw as i32).clamp(1, (lw - ax).max(1));
                    let ah = (rh as i32).clamp(1, (lh - ay).max(1));
                    positioner.set_anchor_rect(ax, ay, aw, ah);
                    positioner.set_anchor(Anchor::BottomLeft);
                    positioner.set_gravity(Gravity::BottomRight);
                } else {
                    let lw = engine_state.logical_width as i32;
                    let lh = engine_state.logical_height as i32;
                    let ax = (px as i32).clamp(0, (lw - 1).max(0));
                    let ay = (py as i32).clamp(0, (lh - 1).max(0));
                    let aw = 1.clamp(1, (lw - ax).max(1));
                    let ah = 1.clamp(1, (lh - ay).max(1));
                    positioner.set_anchor_rect(ax, ay, aw, ah);
                    positioner.set_anchor(Anchor::TopLeft);
                    positioner.set_gravity(Gravity::BottomRight);
                }
                positioner.set_constraint_adjustment(
                    ConstraintAdjustment::SlideX | ConstraintAdjustment::SlideY
                );
                
                let parent_xdg_surface = XdgSurface::xdg_surface(engine_state.window.as_ref().unwrap());
                let sctk_popup = Popup::new(
                    parent_xdg_surface,
                    &positioner,
                    &engine_state.qh,
                    &engine_state.compositor_state,
                    &engine_state.xdg_shell_state,
                ).unwrap();
                sctk_popup.wl_surface().set_buffer_scale(engine_state.scale_factor as i32);
                    
                    let display_ptr = conn.backend().display_id().as_ptr() as *mut std::ffi::c_void;
                    let surface_ptr = sctk_popup.wl_surface().id().as_ptr() as *mut std::ffi::c_void;
                    let wayland_handle = Box::leak(Box::new(WaylandSurfaceHandle {
                        display_ptr,
                        surface_ptr,
                    }));
                    let instance = &engine_state.wgpu_adapter.as_ref().unwrap().instance;
                    let wgpu_surface = instance.create_surface(wayland_handle).expect("failed to create popup wgpu surface");
                    
                    let device = &engine_state.wgpu_adapter.as_ref().unwrap().device;
                    let main_config = &engine_state.wgpu_adapter.as_ref().unwrap().config;
                    
                    let scale_f32 = engine_state.scale_factor as f32;
                    let mut popup_config = main_config.clone();
                    popup_config.width = ((pw * scale_f32) as u32).max(1);
                    popup_config.height = ((ph * scale_f32) as u32).max(1);
                    wgpu_surface.configure(device, &popup_config);
                    
                    let cache = Cache::new(device);
                    let viewport = Viewport::new(device, &cache);
                    
                    engine_state.active_popup = Some(ActivePopup {
                        sctk_popup,
                        wgpu_surface,
                        config: popup_config,
                        logical_width: pw,
                        logical_height: ph,
                        configured: false,
                        viewport,
                        x: px,
                        y: py,
                        vertex_buffer: None,
                    });
                }
        } else {
            if engine_state.active_popup.is_some() {
                engine_state.active_popup = None;
                engine_state.redraw = true;
            }
        }

        if engine_state.redraw && !engine_state.frame_callback_pending {
            engine_state.redraw = false;
            if engine_state.first_configure_received {
                engine_state.render();
            }
        }
    }
    crate::process::cleanup_spawned_processes();
}
