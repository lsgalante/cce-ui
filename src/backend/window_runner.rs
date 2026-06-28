use std::time::Instant;
use smithay_client_toolkit::{
    compositor::{CompositorHandler, CompositorState},
    delegate_compositor, delegate_keyboard, delegate_pointer, delegate_registry,
    delegate_seat, delegate_shm, delegate_xdg_shell, delegate_xdg_window, delegate_output, delegate_xdg_popup,
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
        WaylandSurface,
    },
    shm::{Shm, ShmHandler},
};
use wayland_client::{
    globals::{registry_queue_init, GlobalList},
    protocol::{wl_keyboard, wl_output, wl_pointer, wl_seat, wl_surface, wl_registry, wl_region, wl_callback},
    Connection, QueueHandle, Proxy,
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
}

std::thread_local! {
    static BUFFER_CACHE: std::cell::RefCell<std::collections::HashMap<BufferCacheKey, Buffer>> = std::cell::RefCell::new(std::collections::HashMap::new());
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
    let key = BufferCacheKey {
        text: text.to_string(),
        size_milli: size_key,
        font: family_name.clone(),
    };

    let cached = BUFFER_CACHE.with(|cache| {
        cache.borrow().get(&key).cloned()
    });

    if let Some(buf) = cached {
        return buf;
    }

    let metrics = Metrics::new(physical_size, physical_size * 1.4);
    let mut buf = Buffer::new(fs, metrics);
    let mut attrs = Attrs::new();
    if let Some(ref font_family) = family_name {
        let family = match font_family.as_str() {
            "monospace" => glyphon::Family::Name(crate::layout::get_system_monospace_font()),
            "sans-serif" => glyphon::Family::SansSerif,
            "serif" => glyphon::Family::Serif,
            name => glyphon::Family::Name(name),
        };
        attrs = attrs.family(family);
    }
    buf.set_text(fs, text, attrs, glyphon::Shaping::Advanced);
    buf.shape_until_scroll(fs, true);

    BUFFER_CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        if cache.len() > 2000 {
            cache.clear();
        }
        cache.insert(key, buf.clone());
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
    push_rounded_rect_vertices_corners(x, y, ww, h, r, sw, sh, color, clip_circle, corners, clip_rect, &mut verts);
    verts
}

pub fn push_rounded_rect_vertices_corners(
    x: f32, y: f32, ww: f32, h: f32,
    r: f32,
    sw: f32, sh: f32,
    color: [f32; 4],
    clip_circle: [f32; 3],
    corners: (bool, bool, bool, bool),
    clip_rect: Option<(f32, f32, f32, f32)>,
    out: &mut Vec<Vertex>,
) {
    let r = r.min(ww * 0.5).min(h * 0.5);

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

    if r <= 0.1 || (!corners.0 && !corners.1 && !corners.2 && !corners.3) {
        push_quad(out, x, y, ww, h);
        return;
    }

    // 1. Center rectangle
    push_quad(out, x + r, y, ww - 2.0 * r, h);
    
    // 2. Left rectangle
    push_quad(out, x, y + r, r, h - 2.0 * r);
    
    // 3. Right rectangle
    push_quad(out, x + ww - r, y + r, r, h - 2.0 * r);

    // 4. Four corners
    let corner_configs = [
        // Top-left
        (corners.0, x, y, x + r, y + r, std::f32::consts::PI, 1.5 * std::f32::consts::PI),
        // Top-right
        (corners.1, x + ww - r, y, x + ww - r, y + r, 1.5 * std::f32::consts::PI, 2.0 * std::f32::consts::PI),
        // Bottom-right
        (corners.2, x + ww - r, y + h - r, x + ww - r, y + h - r, 0.0, 0.5 * std::f32::consts::PI),
        // Bottom-left
        (corners.3, x, y + h - r, x + r, y + h - r, 0.5 * std::f32::consts::PI, std::f32::consts::PI),
    ];

    let segments = 16;
    for &(is_rounded, sqx, sqy, cx, cy, start, end) in &corner_configs {
        if is_rounded {
            for i in 0..segments {
                let theta1 = start + (i as f32) * (end - start) / (segments as f32);
                let theta2 = start + ((i + 1) as f32) * (end - start) / (segments as f32);
                
                let x0 = clamp_x(cx);
                let y0 = clamp_y(cy);
                let x1 = clamp_x(cx + r * theta1.cos());
                let y1 = clamp_y(cy + r * theta1.sin());
                let x2 = clamp_x(cx + r * theta2.cos());
                let y2 = clamp_y(cy + r * theta2.sin());
                
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
        } else {
            push_quad(out, sqx, sqy, r, r);
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
    rounded_rect_vertices_corners(x, y, ww, h, r, sw, sh, color, clip_circle, (true, true, true, true), None)
}

pub fn push_rounded_rect_vertices(
    x: f32, y: f32, ww: f32, h: f32,
    r: f32,
    sw: f32, sh: f32,
    color: [f32; 4],
    clip_circle: [f32; 3],
    out: &mut Vec<Vertex>,
) {
    push_rounded_rect_vertices_corners(x, y, ww, h, r, sw, sh, color, clip_circle, (true, true, true, true), None, out);
}

pub fn plate_bevel_vertices(
    x: f32, y: f32, ww: f32, h: f32,
    r: f32,
    t: f32,
    sw: f32, sh: f32,
    clip_circle: [f32; 3],
) -> Vec<Vertex> {
    let mut verts = Vec::new();
    push_plate_bevel_vertices(x, y, ww, h, r, t, sw, sh, clip_circle, &mut verts);
    verts
}

pub fn push_plate_bevel_vertices(
    x: f32, y: f32, ww: f32, h: f32,
    r: f32,
    t: f32,
    sw: f32, sh: f32,
    clip_circle: [f32; 3],
    out: &mut Vec<Vertex>,
) {
    let r = r.min(ww * 0.5).min(h * 0.5);
    let highlight_color = [1.0, 1.0, 1.0, 0.15];
    let shadow_color = [0.0, 0.0, 0.0, 0.25];

    out.extend_from_slice(&quad_vertices_with_clip(x + r, y, ww - 2.0 * r, t, sw, sh, highlight_color, clip_circle));
    out.extend_from_slice(&quad_vertices_with_clip(x, y + r, t, h - 2.0 * r, sw, sh, highlight_color, clip_circle));
    out.extend_from_slice(&quad_vertices_with_clip(x + r, y + h - t, ww - 2.0 * r, t, sw, sh, shadow_color, clip_circle));
    out.extend_from_slice(&quad_vertices_with_clip(x + ww - t, y + r, t, h - 2.0 * r, sw, sh, shadow_color, clip_circle));

    let segments = 16;

    push_arc_background_vertices(
        x + r, y + r, r, t,
        std::f32::consts::PI, 1.5 * std::f32::consts::PI,
        sw, sh, highlight_color, segments, clip_circle,
        out,
    );

    push_arc_background_vertices(
        x + ww - r, y + r, r, t,
        1.5 * std::f32::consts::PI, 1.75 * std::f32::consts::PI,
        sw, sh, highlight_color, segments / 2, clip_circle,
        out,
    );
    push_arc_background_vertices(
        x + ww - r, y + r, r, t,
        1.75 * std::f32::consts::PI, 2.0 * std::f32::consts::PI,
        sw, sh, shadow_color, segments / 2, clip_circle,
        out,
    );

    push_arc_background_vertices(
        x + ww - r, y + h - r, r, t,
        0.0, 0.5 * std::f32::consts::PI,
        sw, sh, shadow_color, segments, clip_circle,
        out,
    );

    push_arc_background_vertices(
        x + r, y + h - r, r, t,
        0.5 * std::f32::consts::PI, 0.75 * std::f32::consts::PI,
        sw, sh, shadow_color, segments / 2, clip_circle,
        out,
    );
    push_arc_background_vertices(
        x + r, y + h - r, r, t,
        0.75 * std::f32::consts::PI, std::f32::consts::PI,
        sw, sh, highlight_color, segments / 2, clip_circle,
        out,
    );
}

pub fn push_plate_solid_border_vertices(
    x: f32, y: f32, ww: f32, h: f32,
    r: f32,
    t: f32,
    sw: f32, sh: f32,
    color: [f32; 4],
    clip_circle: [f32; 3],
    out: &mut Vec<Vertex>,
) {
    let r = r.min(ww * 0.5).min(h * 0.5);
    out.extend_from_slice(&quad_vertices_with_clip(x + r, y, ww - 2.0 * r, t, sw, sh, color, clip_circle));
    out.extend_from_slice(&quad_vertices_with_clip(x, y + r, t, h - 2.0 * r, sw, sh, color, clip_circle));
    out.extend_from_slice(&quad_vertices_with_clip(x + r, y + h - t, ww - 2.0 * r, t, sw, sh, color, clip_circle));
    out.extend_from_slice(&quad_vertices_with_clip(x + ww - t, y + r, t, h - 2.0 * r, sw, sh, color, clip_circle));

    let segments = 16;

    push_arc_background_vertices(
        x + r, y + r, r, t,
        std::f32::consts::PI, 1.5 * std::f32::consts::PI,
        sw, sh, color, segments, clip_circle,
        out,
    );

    push_arc_background_vertices(
        x + ww - r, y + r, r, t,
        1.5 * std::f32::consts::PI, 2.0 * std::f32::consts::PI,
        sw, sh, color, segments, clip_circle,
        out,
    );

    push_arc_background_vertices(
        x + ww - r, y + h - r, r, t,
        0.0, 0.5 * std::f32::consts::PI,
        sw, sh, color, segments, clip_circle,
        out,
    );

    push_arc_background_vertices(
        x + r, y + h - r, r, t,
        0.5 * std::f32::consts::PI, std::f32::consts::PI,
        sw, sh, color, segments, clip_circle,
        out,
    );
}

pub fn widget_vertices(w: &dyn crate::widget::Element, sw: f32, sh: f32, clip_circle: [f32; 3]) -> Vec<Vertex> {
    let mut verts = Vec::new();
    push_widget_vertices(w, sw, sh, clip_circle, &mut verts);
    verts
}

pub fn push_widget_vertices(w: &dyn crate::widget::Element, sw: f32, sh: f32, clip_circle: [f32; 3], out: &mut Vec<Vertex>) {
    let (x, y, ww, h) = w.rect();
    let corners = w.rounded_corners();
    if corners != (false, false, false, false) {
        push_rounded_rect_vertices_corners(x, y, ww, h, w.corner_radius(), sw, sh, w.color(), clip_circle, corners, None, out);
    } else {
        out.extend_from_slice(&quad_vertices_with_clip(x, y, ww, h, sw, sh, w.color(), clip_circle));
    }

    if let Some((color, thickness)) = w.solid_border() {
        push_plate_solid_border_vertices(x, y, ww, h, w.corner_radius(), thickness, sw, sh, color, clip_circle, out);
    }
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
    let target_w = get_child_widget_for_quad(w, qx, qy, qw, qh);
    let corners = target_w.rounded_corners();
    if corners == (false, false, false, false) {
        out.extend_from_slice(&quad_vertices_with_clip(qx, qy, qw, qh, sw, sh, qc, clip_circle));
        if let Some((color, thickness)) = target_w.solid_border() {
            let (wx, wy, ww, wh) = target_w.rect();
            if (qx - wx).abs() < 0.1 && (qy - wy).abs() < 0.1 && (qw - ww).abs() < 0.1 && (qh - wh).abs() < 0.1 {
                push_plate_solid_border_vertices(qx, qy, qw, qh, target_w.corner_radius(), thickness, sw, sh, color, clip_circle, out);
            }
        }
        return;
    }

    let (wx, mut wy, ww, mut wh) = target_w.rect();
    let top_room = crate::widget::label_offset(target_w);
    wy += top_room;
    wh -= top_room;
    let extra_corners = (
        corners.0 && qx <= wx + 1.5 && qy <= wy + 1.5,
        corners.1 && qx + qw >= wx + ww - 1.5 && qy <= wy + 1.5,
        corners.2 && qx + qw >= wx + ww - 1.5 && qy + qh >= wy + wh - 1.5,
        corners.3 && qx <= wx + 1.5 && qy + qh >= wy + wh - 1.5,
    );

    push_rounded_rect_vertices_corners(qx, qy, qw, qh, target_w.corner_radius(), sw, sh, qc, clip_circle, extra_corners, None, out);

    if let Some((color, thickness)) = target_w.solid_border() {
        let (rx, mut ry, rw, mut rh) = target_w.rect();
        let top = crate::widget::label_offset(target_w);
        ry += top;
        rh -= top;
        if (qx - rx).abs() < 0.1 && (qy - ry).abs() < 0.1 && (qw - rw).abs() < 0.1 && (qh - rh).abs() < 0.1 {
            push_plate_solid_border_vertices(qx, qy, qw, qh, target_w.corner_radius(), thickness, sw, sh, color, clip_circle, out);
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
    let target_w = get_child_widget_for_quad(w, qx, qy, qw, qh);
    let corners = target_w.rounded_corners();
    if corners == (false, false, false, false) {
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
                push_plate_solid_border_vertices(qx, qy, qw, qh, target_w.corner_radius(), thickness, sw, sh, color, clip_circle, out);
            }
        }
        return;
    }

    let (wx, mut wy, ww, mut wh) = target_w.rect();
    let top_room = crate::widget::label_offset(target_w);
    wy += top_room;
    wh -= top_room;
    let extra_corners = (
        corners.0 && qx <= wx + 1.5 && qy <= wy + 1.5,
        corners.1 && qx + qw >= wx + ww - 1.5 && qy <= wy + 1.5,
        corners.2 && qx + qw >= wx + ww - 1.5 && qy + qh >= wy + wh - 1.5,
        corners.3 && qx <= wx + 1.5 && qy + qh >= wy + wh - 1.5,
    );

    push_rounded_rect_vertices_corners(qx, qy, qw, qh, target_w.corner_radius(), sw, sh, qc, clip_circle, extra_corners, Some(clip), out);

    if let Some((color, thickness)) = target_w.solid_border() {
        let (rx, mut ry, rw, mut rh) = target_w.rect();
        let top = crate::widget::label_offset(target_w);
        ry += top;
        rh -= top;
        if (qx - rx).abs() < 0.1 && (qy - ry).abs() < 0.1 && (qw - rw).abs() < 0.1 && (qh - rh).abs() < 0.1 {
            push_plate_solid_border_vertices(qx, qy, qw, qh, target_w.corner_radius(), thickness, sw, sh, color, clip_circle, out);
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
    fn update(&mut self, msg: Self::Message, needs_rebuild: &mut bool, exit: &mut bool);
    fn tick(&mut self, dt: f32, needs_rebuild: &mut bool);
    fn view(&mut self, quads: &mut Vec<(f32, f32, f32, f32, [f32; 4])>, size: LogicalSize, scale: f64);
    fn view_rounded_quads(&mut self, _quads: &mut Vec<(f32, f32, f32, f32, f32, [f32; 4], (bool, bool, bool, bool))>, _size: LogicalSize, _scale: f64) {}
    fn view_vectors(&mut self, _vectors: &mut Vec<(f32, f32, f32, f32, f32, [f32; 4], LineCap)>, _size: LogicalSize, _scale: f64) {}
    fn overlay_quads(&mut self, _quads: &mut Vec<(f32, f32, f32, f32, [f32; 4])>, _size: LogicalSize, _scale: f64) {}
    fn text_items(&self) -> &[TextItem];
    fn render_popovers(&self, _pc: &mut dyn crate::layout::RenderTarget) {}
    fn input_regions(&self) -> Option<Vec<(i32, i32, i32, i32)>> {
        None
    }
    
    fn ui_context(&self) -> Option<&crate::context::UiContext> {
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
        let overlay_rects: Vec<(f32, f32, f32, f32)> = Vec::new();

        self.text_items().iter().map(|ti| {
            let mut item_bounds = if let Some([l, t, r, b]) = ti.bounds {
                TextBounds {
                    left: (l * scale_f32).round() as i32,
                    top: (t * scale_f32).round() as i32,
                    right: (r * scale_f32).round() as i32,
                    bottom: (b * scale_f32).round() as i32,
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
    
    fn handle_pointer_move(&mut self, pos: LogicalPosition, needs_rebuild: &mut bool);
    fn handle_mouse_input(&mut self, button: MouseButton, state: ElementState, pos: LogicalPosition, needs_rebuild: &mut bool) -> Option<Self::Message>;
    fn handle_mouse_wheel(&mut self, delta: &MouseScrollDelta, pos: LogicalPosition, needs_rebuild: &mut bool);
    fn handle_key_input(&mut self, event: &KeyEvent, needs_rebuild: &mut bool) -> Option<Self::Message>;
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
    pub shm_state: Shm,
    pub seat_state: SeatState,
    pub output_state: OutputState,
    pub seats: Vec<wl_seat::WlSeat>,
    pub pointer: Option<ThemedPointer>,
    pub keyboard: Option<wl_keyboard::WlKeyboard>,
    
    pub window: Option<XdgWindow>,
    pub surface: Option<wl_surface::WlSurface>,
    
    pub inner: A,
    
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
    pub pressed_key: Option<PressedKey>,
    pub sender: calloop::channel::Sender<A::Message>,
    pub active_popup: Option<ActivePopup>,
    pub current_cursor_icon: Option<CursorIcon>,
    pub qh: QueueHandle<EngineState<A>>,
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
        self.inner.view(&mut quads, LogicalSize::new(logical_w, logical_h), scale_factor);

        if let Some(ref surface) = self.surface {
            if let Some(regions) = self.inner.input_regions() {
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
        self.inner.view_rounded_quads(&mut rounded_quads, LogicalSize::new(logical_w, logical_h), scale_factor);
        
        let mut vectors = Vec::new();
        self.inner.view_vectors(&mut vectors, LogicalSize::new(logical_w, logical_h), scale_factor);
        
        let adapter = self.wgpu_adapter.as_mut().unwrap();
        let render_pipeline = self.render_pipeline.as_ref().unwrap();
        
        // 1. Build and upload vertex buffer
        let mut verts = Vec::new();
        for &(qx, qy, qw, qh, qc) in &quads {
            verts.extend(quad_vertices(qx, qy, qw, qh, logical_w, logical_h, qc));
        }
        for &(qx, qy, qw, qh, qr, qc, qcorners) in &rounded_quads {
            if qr > 0.1 {
                push_rounded_rect_vertices_corners(qx, qy, qw, qh, qr, logical_w, logical_h, qc, [0.0, 0.0, 0.0], qcorners, None, &mut verts);
            } else {
                verts.extend(quad_vertices(qx, qy, qw, qh, logical_w, logical_h, qc));
            }
        }
        for &(vx1, vy1, vx2, vy2, vthickness, vcolor, vcap) in &vectors {
            verts.extend(vector_vertices(vx1, vy1, vx2, vy2, vthickness, logical_w, logical_h, vcolor, vcap));
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
        self.inner.overlay_quads(&mut overlay_quads, LogicalSize::new(logical_w, logical_h), scale_factor);
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
        let areas = self.inner.text_areas(scale_f32, bounds);
        
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
            let cc = self.inner.clear_color();
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
                pass.draw(0..self.vertex_count, 0..1);
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
                self.inner.render_popovers(&mut collector);
 
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
    ) {}
    
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
            self.resize(width as f32, height as f32);
        } else {
            let settings = self.inner.settings();
            let w = settings.width as f32;
            let h = settings.height as f32;
            self.resize(w, h);
        }
        self.redraw = true;
        self.frame_callback_pending = false;
        self.first_configure_received = true;
    }
    
    fn request_close(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, _window: &XdgWindow) {
        self.exit = true;
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
            let surface = self.compositor_state.create_surface(qh);
            let themed_pointer = self.seat_state.get_pointer_with_theme(
                qh,
                &seat,
                self.shm_state.wl_shm(),
                surface,
                ThemeSpec::System,
            ).unwrap();
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
            
            match &event.kind {
                PointerEventKind::Enter { .. } => {
                    let is_status_bar = self.inner.settings().app_id == "cce-status-interface";
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
                    self.inner.handle_pointer_move(LogicalPosition::new(-10000.0, -10000.0), &mut rebuild);
                    if rebuild {
                        self.redraw = true;
                    }
                }
                PointerEventKind::Motion { .. } => {
                    let mut rebuild = false;
                    self.inner.handle_pointer_move(LogicalPosition::new(lx, ly), &mut rebuild);
                    if rebuild {
                        self.redraw = true;
                    }

                    let is_status_bar = self.inner.settings().app_id == "cce-status-interface";
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
                    let is_status_bar = self.inner.settings().app_id == "cce-status-interface";
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
                        if ly >= border && ly < 32.0 && lx < self.logical_width - 70.0 {
                            should_move = true;
                        } else if self.inner.is_movable_backplate_at(lx, ly) {
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
                    if let Some(msg) = self.inner.handle_mouse_input(btn, ElementState::Pressed, LogicalPosition::new(lx, ly), &mut rebuild) {
                        let mut update_rebuild = false;
                        self.inner.update(msg, &mut update_rebuild, &mut self.exit);
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
                    if let Some(msg) = self.inner.handle_mouse_input(btn, ElementState::Released, LogicalPosition::new(lx, ly), &mut rebuild) {
                        let mut update_rebuild = false;
                        self.inner.update(msg, &mut update_rebuild, &mut self.exit);
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
            self.inner.handle_mouse_wheel(&delta, LogicalPosition::new(last_lx, last_ly), &mut rebuild);
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

        let mut rebuild = false;
        if let Some(msg) = self.inner.handle_key_input(&custom_event, &mut rebuild) {
            let mut update_rebuild = false;
            self.inner.update(msg, &mut update_rebuild, &mut self.exit);
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

pub fn run<A: Application>() {
    let conn = Connection::connect_to_env().unwrap();
    let (globals, mut event_queue) = registry_queue_init(&conn).unwrap();
    let qh = event_queue.handle();

    let compositor_state = CompositorState::bind(&globals, &qh).unwrap();
    let xdg_shell_state = XdgShell::bind(&globals, &qh).unwrap();
    let shm_state = Shm::bind(&globals, &qh).unwrap();
    let seat_state = SeatState::new(&globals, &qh);
    let output_state = OutputState::new(&globals, &qh);

    let (sender, channel) = calloop::channel::channel::<A::Message>();

    let inner = A::new(&qh, sender.clone());
    let settings = inner.settings();
    crate::scale::set_app_id(settings.app_id.clone());

    let mut engine_state = EngineState {
        registry_state: RegistryState::new(&globals),
        compositor_state,
        xdg_shell_state,
        shm_state,
        seat_state,
        output_state,
        seats: Vec::new(),
        pointer: None,
        keyboard: None,
        window: None,
        surface: None,
        inner,
        wgpu_adapter: None,
        render_pipeline: None,
        vertex_buffer: None,
        vertex_count: 0,
        overlay_vertex_buffer: None,
        overlay_vertex_count: 0,
        scale_factor: 1.0,
        logical_width: settings.width as f32,
        logical_height: settings.height as f32,
        exit: false,
        redraw: false,
        frame_callback_pending: false,
        first_configure_received: false,
        ctrl_pressed: false,
        shift_pressed: false,
        pressed_key: None,
        sender,
        active_popup: None,
        current_cursor_icon: None,
        qh: qh.clone(),
    };

    event_queue.roundtrip(&mut engine_state).unwrap();

    let scale = detect_scale_factor(&engine_state.output_state);
    engine_state.scale_factor = scale;

    let surface = engine_state.compositor_state.create_surface(&qh);
    surface.set_buffer_scale(scale as i32);

    if settings.app_id == "cce-status-interface" {
        let compositor = engine_state.compositor_state.wl_compositor();
        let region = compositor.create_region(&qh, ());
        region.add(0, 0, settings.width as i32, settings.height as i32);
        surface.set_input_region(Some(&region));
        region.destroy();
    }

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
    engine_state.surface = Some(surface);

    pollster::block_on(engine_state.init_gpu(&conn, settings.width as f32, settings.height as f32));

    let mut event_loop = EventLoop::try_new().unwrap();
    let loop_handle = event_loop.handle();
    WaylandSource::new(conn.clone(), event_queue).insert(loop_handle.clone()).unwrap();

    loop_handle.insert_source(channel, |event, _metadata, app_state: &mut EngineState<A>| {
        if let calloop::channel::Event::Msg(msg) = event {
            let mut rebuild = false;
            app_state.inner.update(msg, &mut rebuild, &mut app_state.exit);
            if rebuild {
                app_state.redraw = true;
            }
        }
    }).unwrap();

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
        engine_state.inner.tick(dt, &mut rebuild);
        if rebuild {
            engine_state.redraw = true;
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
                    let mut key_rebuild = false;
                    if let Some(msg) = engine_state.inner.handle_key_input(&custom_event, &mut key_rebuild) {
                        let mut update_rebuild = false;
                        engine_state.inner.update(msg, &mut update_rebuild, &mut engine_state.exit);
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
        let current_title = engine_state.inner.settings().title;
        if current_title != last_title {
            if let Some(ref window) = engine_state.window {
                window.set_title(&current_title);
                window.commit();
            }
            last_title = current_title;
        }

        let active_popovers = crate::widget::popovers::get_active();
        let context_menu_visible = crate::widget::context_menu::is_visible();
        if !active_popovers.is_empty() || context_menu_visible {
            let (px, py, mut pw, mut ph, is_context_menu) = if !active_popovers.is_empty() {
                let popover_widget = unsafe { &*active_popovers[0] };
                let (x, y, w, h) = popover_widget.popover_rect().unwrap();
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
            if engine_state.active_popup.is_none() {
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
}
