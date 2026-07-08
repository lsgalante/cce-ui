use cce_ui::widget::{
    Button, Checkbox, ContentBg, Header, Panel, ProgressBar, RangeSlider, Sidebar, Slider, Spinbox,
    StatusBar, TextLabel, Toggle, Element, JsonLayoutWidget, JsonLayoutConfig,
};

use glyphon::{
    Attrs, Buffer, Cache, FontSystem, Metrics, Resolution, SwashCache, TextArea, TextAtlas,
    TextBounds, TextRenderer, Viewport,
};

use smithay_client_toolkit::{
    compositor::{CompositorHandler, CompositorState},
    delegate_compositor, delegate_keyboard, delegate_pointer, delegate_registry,
    delegate_seat, delegate_shm, delegate_xdg_shell, delegate_xdg_window, delegate_output,
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
    globals::registry_queue_init,
    protocol::{wl_keyboard, wl_output, wl_pointer, wl_seat, wl_surface},
    Connection, QueueHandle, Proxy,
};
use calloop::EventLoop;
use calloop_wayland_source::WaylandSource;

#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Vertex {
    position: [f32; 2],
    color: [f32; 4],
    clip_circle: [f32; 3],
}

impl Vertex {
    const ATTRIBS: [wgpu::VertexAttribute; 3] = wgpu::vertex_attr_array![
        0 => Float32x2,
        1 => Float32x4,
        2 => Float32x3,
    ];

    fn desc() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Vertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &Self::ATTRIBS,
        }
    }
}

fn circle_vertices(
    cx: f32, cy: f32, r: f32,
    sw: f32, sh: f32,
    color: [f32; 4],
    segments: usize,
) -> Vec<Vertex> {
    let mut verts = Vec::with_capacity(segments * 3);
    for i in 0..segments {
        let a1 = i as f32 * 2.0 * std::f32::consts::PI / segments as f32;
        let a2 = (i + 1) as f32 * 2.0 * std::f32::consts::PI / segments as f32;
        let x1 = cx + r * a1.cos();
        let y1 = cy + r * a1.sin();
        let x2 = cx + r * a2.cos();
        let y2 = cy + r * a2.sin();

        verts.push(Vertex {
            position: [(cx / sw) * 2.0 - 1.0, 1.0 - (cy / sh) * 2.0],
            color,
            clip_circle: [0.0, 0.0, 0.0],
        });
        verts.push(Vertex {
            position: [(x1 / sw) * 2.0 - 1.0, 1.0 - (y1 / sh) * 2.0],
            color,
            clip_circle: [0.0, 0.0, 0.0],
        });
        verts.push(Vertex {
            position: [(x2 / sw) * 2.0 - 1.0, 1.0 - (y2 / sh) * 2.0],
            color,
            clip_circle: [0.0, 0.0, 0.0],
        });
    }
    verts
}

fn quad_vertices(
    x: f32, y: f32, w: f32, h: f32,
    surface_w: f32, surface_h: f32,
    color: [f32; 4],
) -> [Vertex; 6] {
    let x0 = (x / surface_w) * 2.0 - 1.0;
    let y0 = 1.0 - (y / surface_h) * 2.0;
    let x1 = ((x + w) / surface_w) * 2.0 - 1.0;
    let y1 = 1.0 - ((y + h) / surface_h) * 2.0;

    [
        Vertex { position: [x0, y0], color, clip_circle: [0.0, 0.0, 0.0] },
        Vertex { position: [x1, y0], color, clip_circle: [0.0, 0.0, 0.0] },
        Vertex { position: [x0, y1], color, clip_circle: [0.0, 0.0, 0.0] },
        Vertex { position: [x1, y0], color, clip_circle: [0.0, 0.0, 0.0] },
        Vertex { position: [x1, y1], color, clip_circle: [0.0, 0.0, 0.0] },
        Vertex { position: [x0, y1], color, clip_circle: [0.0, 0.0, 0.0] },
    ]
}

fn rounded_rect_vertices_corners(
    x: f32, y: f32, ww: f32, h: f32,
    r: f32,
    sw: f32, sh: f32,
    color: [f32; 4],
    corners: (bool, bool, bool, bool),
) -> Vec<Vertex> {
    let mut verts = Vec::new();
    let r = r.min(ww * 0.5).min(h * 0.5);

    let push_quad = |verts: &mut Vec<Vertex>, qx: f32, qy: f32, qw: f32, qh: f32| {
        let x0 = qx;
        let y0 = qy;
        let x1 = qx + qw;
        let y1 = qy + qh;
        
        let ndc_x0 = (x0 / sw) * 2.0 - 1.0;
        let ndc_y0 = 1.0 - (y0 / sh) * 2.0;
        let ndc_x1 = (x1 / sw) * 2.0 - 1.0;
        let ndc_y1 = 1.0 - (y1 / sh) * 2.0;
        
        let clip_circle = [0.0, 0.0, 0.0];
        verts.push(Vertex { position: [ndc_x0, ndc_y0], color, clip_circle });
        verts.push(Vertex { position: [ndc_x1, ndc_y0], color, clip_circle });
        verts.push(Vertex { position: [ndc_x0, ndc_y1], color, clip_circle });
        verts.push(Vertex { position: [ndc_x1, ndc_y0], color, clip_circle });
        verts.push(Vertex { position: [ndc_x1, ndc_y1], color, clip_circle });
        verts.push(Vertex { position: [ndc_x0, ndc_y1], color, clip_circle });
    };

    if r <= 0.1 || (!corners.0 && !corners.1 && !corners.2 && !corners.3) {
        push_quad(&mut verts, x, y, ww, h);
        return verts;
    }

    push_quad(&mut verts, x + r, y, ww - 2.0 * r, h);
    push_quad(&mut verts, x, y + r, r, h - 2.0 * r);
    push_quad(&mut verts, x + ww - r, y + r, r, h - 2.0 * r);

    let corner_configs = [
        (corners.0, x, y, x + r, y + r, std::f32::consts::PI, 1.5 * std::f32::consts::PI),
        (corners.1, x + ww - r, y, x + ww - r, y + r, 1.5 * std::f32::consts::PI, 2.0 * std::f32::consts::PI),
        (corners.2, x + ww - r, y + h - r, x + ww - r, y + h - r, 0.0, 0.5 * std::f32::consts::PI),
        (corners.3, x, y + h - r, x + r, y + h - r, 0.5 * std::f32::consts::PI, std::f32::consts::PI),
    ];

    let segments = 16;
    for &(is_rounded, sqx, sqy, cx, cy, start, end) in &corner_configs {
        if is_rounded {
            for i in 0..segments {
                let theta1 = start + (i as f32) * (end - start) / (segments as f32);
                let theta2 = start + ((i + 1) as f32) * (end - start) / (segments as f32);
                
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
                
                let clip_circle = [0.0, 0.0, 0.0];
                verts.push(Vertex { position: [ndc_x0, ndc_y0], color, clip_circle });
                verts.push(Vertex { position: [ndc_x1, ndc_y1], color, clip_circle });
                verts.push(Vertex { position: [ndc_x2, ndc_y2], color, clip_circle });
            }
        } else {
            push_quad(&mut verts, sqx, sqy, r, r);
        }
    }

    verts
}

#[allow(dead_code)]
fn rounded_rect_vertices(
    x: f32, y: f32, ww: f32, h: f32,
    r: f32,
    sw: f32, sh: f32,
    color: [f32; 4],
) -> Vec<Vertex> {
    rounded_rect_vertices_corners(x, y, ww, h, r, sw, sh, color, (true, true, true, true))
}

fn widget_vertices(w: &dyn Element, sw: f32, sh: f32) -> Vec<Vertex> {
    let (x, y, ww, h) = w.rect();
    let corners = w.rounded_corners();
    if corners != (false, false, false, false) {
        rounded_rect_vertices_corners(x, y, ww, h, w.corner_radius(), sw, sh, w.color(), corners)
    } else {
        quad_vertices(x, y, ww, h, sw, sh, w.color()).to_vec()
    }
}

fn extra_quad_vertices(
    w: &dyn Element,
    qx: f32, qy: f32, qw: f32, qh: f32,
    sw: f32, sh: f32,
    qc: [f32; 4],
) -> Vec<Vertex> {
    let mut corners = w.rounded_corners();
    let mut r = w.corner_radius();
    let (mut wx, mut wy, mut ww, mut wh) = w.rect();
    let mut top_room = cce_ui::widget::label_offset(w);

    if let Some(mc) = w.as_any().downcast_ref::<cce_ui::widget::input::MultiControl>() {
        // check add_button
        let (bx, by, b_w, b_h) = mc.add_button.rect();
        if qx >= bx - 0.1 && qx + qw <= bx + b_w + 0.1 && qy >= by - 0.1 && qy + qh <= by + b_h + 0.1 {
            corners = mc.add_button.rounded_corners();
            r = mc.add_button.corner_radius();
            wx = bx;
            wy = by;
            ww = b_w;
            wh = b_h;
            top_room = cce_ui::widget::label_offset(&mc.add_button);
        }
        for row in &mc.rows {
            // key_input
            let (kx, ky, kw, kh) = row.key_input.rect();
            if qx >= kx - 0.1 && qx + qw <= kx + kw + 0.1 && qy >= ky - 0.1 && qy + qh <= ky + kh + 0.1 {
                corners = row.key_input.rounded_corners();
                r = row.key_input.corner_radius();
                wx = kx;
                wy = ky;
                ww = kw;
                wh = kh;
                top_room = cce_ui::widget::label_offset(&row.key_input);
            }
            // type_dropdown
            let (tx, ty, tw, th) = row.type_dropdown.rect();
            if qx >= tx - 0.1 && qx + qw <= tx + tw + 0.1 && qy >= ty - 0.1 && qy + qh <= ty + th + 0.1 {
                corners = row.type_dropdown.rounded_corners();
                r = row.type_dropdown.corner_radius();
                wx = tx;
                wy = ty;
                ww = tw;
                wh = th;
                top_room = cce_ui::widget::label_offset(&row.type_dropdown);
            }
            // remove_button
            let (rx, ry, rw, rh) = row.remove_button.rect();
            if qx >= rx - 0.1 && qx + qw <= rx + rw + 0.1 && qy >= ry - 0.1 && qy + qh <= ry + rh + 0.1 {
                corners = row.remove_button.rounded_corners();
                r = row.remove_button.corner_radius();
                wx = rx;
                wy = ry;
                ww = rw;
                wh = rh;
                top_room = cce_ui::widget::label_offset(&row.remove_button);
            }
            // value_widget
            let (vx, vy, v_w, v_h) = row.value_widget.rect();
            if qx >= vx - 0.1 && qx + qw <= vx + v_w + 0.1 && qy >= vy - 0.1 && qy + qh <= vy + v_h + 0.1 {
                let (sub_corners, sub_radius, sub_top_room) = match &row.value_widget {
                    cce_ui::widget::input::InstancedWidget::TextBox(w) => (w.rounded_corners(), w.corner_radius(), cce_ui::widget::label_offset(w)),
                    cce_ui::widget::input::InstancedWidget::Spinbox(w) => (w.rounded_corners(), w.corner_radius(), cce_ui::widget::label_offset(w)),
                    cce_ui::widget::input::InstancedWidget::Toggle(w) => (w.rounded_corners(), w.corner_radius(), cce_ui::widget::label_offset(w)),
                    cce_ui::widget::input::InstancedWidget::Slider(w) => (w.rounded_corners(), w.corner_radius(), cce_ui::widget::label_offset(w)),
                };
                corners = sub_corners;
                r = sub_radius;
                wx = vx;
                wy = vy;
                ww = v_w;
                wh = v_h;
                top_room = sub_top_room;
            }
        }
    }

    if let Some(kc) = w.as_any().downcast_ref::<cce_ui::widget::input::KeybindsControl>() {
        // check add_button
        let (bx, by, b_w, b_h) = kc.add_button.rect();
        if qx >= bx - 0.1 && qx + qw <= bx + b_w + 0.1 && qy >= by - 0.1 && qy + qh <= by + b_h + 0.1 {
            corners = kc.add_button.rounded_corners();
            r = kc.add_button.corner_radius();
            wx = bx;
            wy = by;
            ww = b_w;
            wh = b_h;
            top_room = cce_ui::widget::label_offset(&kc.add_button);
        }
        for row in &kc.rows {
            // key_input
            let (kx, ky, kw, kh) = row.key_input.rect();
            if qx >= kx - 0.1 && qx + qw <= kx + kw + 0.1 && qy >= ky - 0.1 && qy + qh <= ky + kh + 0.1 {
                corners = row.key_input.rounded_corners();
                r = row.key_input.corner_radius();
                wx = kx;
                wy = ky;
                ww = kw;
                wh = kh;
                top_room = cce_ui::widget::label_offset(&row.key_input);
            }
            // cmd_input
            let (cx, cy, cw, ch) = row.cmd_input.rect();
            if qx >= cx - 0.1 && qx + qw <= cx + cw + 0.1 && qy >= cy - 0.1 && qy + qh <= cy + ch + 0.1 {
                corners = row.cmd_input.rounded_corners();
                r = row.cmd_input.corner_radius();
                wx = cx;
                wy = cy;
                ww = cw;
                wh = ch;
                top_room = cce_ui::widget::label_offset(&row.cmd_input);
            }
            // remove_button
            let (rx, ry, rw, rh) = row.remove_button.rect();
            if qx >= rx - 0.1 && qx + qw <= rx + rw + 0.1 && qy >= ry - 0.1 && qy + qh <= ry + rh + 0.1 {
                corners = row.remove_button.rounded_corners();
                r = row.remove_button.corner_radius();
                wx = rx;
                wy = ry;
                ww = rw;
                wh = rh;
                top_room = cce_ui::widget::label_offset(&row.remove_button);
            }
        }
    }

    if corners == (false, false, false, false) {
        return quad_vertices(qx, qy, qw, qh, sw, sh, qc).to_vec();
    }

    wy += top_room;
    wh -= top_room;
    let extra_corners = (
        corners.0 && qx <= wx + 1.5 && qy <= wy + 1.5,
        corners.1 && qx + qw >= wx + ww - 1.5 && qy <= wy + 1.5,
        corners.2 && qx + qw >= wx + ww - 1.5 && qy + qh >= wy + wh - 1.5,
        corners.3 && qx <= wx + 1.5 && qy + qh >= wy + wh - 1.5,
    );

    rounded_rect_vertices_corners(qx, qy, qw, qh, r, sw, sh, qc, extra_corners)
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

fn make_text_buffer(font_system: &mut FontSystem, text: &str, size: f32) -> Buffer {
    make_text_buffer_with_font(font_system, text, size, None)
}

fn make_text_buffer_with_font(font_system: &mut FontSystem, text: &str, size: f32, font: Option<&str>) -> Buffer {
    let scale = cce_ui::scale::scale_factor();
    let mut font_size = size;
    let mut family_name = None;

    if let Some(font_str) = font {
        let (parsed_family, parsed_size) = cce_ui::layout::parse_font_string(font_str);
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

    let metrics = Metrics::new(physical_size, physical_size * 1.0);
    let mut buffer = Buffer::new(font_system, metrics);
    let mut attrs = Attrs::new();
    if let Some(font_name) = family_name.as_deref() {
        let family = match font_name {
            "monospace" => glyphon::Family::Name(cce_ui::layout::get_system_monospace_font()),
            "sans-serif" => glyphon::Family::SansSerif,
            "serif" => glyphon::Family::Serif,
            _ => glyphon::Family::Name(font_name),
        };
        attrs = attrs.family(family);
    }
    buffer.set_text(font_system, text, attrs, glyphon::Shaping::Advanced);
    buffer.shape_until_scroll(font_system, true);

    BUFFER_CACHE.with(|cache| {
        cache.borrow_mut().insert(key, buffer.clone());
    });

    buffer
}


struct State {
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    render_pipeline: wgpu::RenderPipeline,
    vertex_buffer: wgpu::Buffer,
    vertex_count: u32,

    widgets: Vec<Box<dyn Element>>,
    positions: Vec<(f32, f32, f32, f32)>,

    font_system: FontSystem,
    swash_cache: SwashCache,
    text_atlas: TextAtlas,
    text_renderer: TextRenderer,
    text_viewport: Viewport,

    label_buffer: Buffer,
    status_buffer: Buffer,

    drag_widget: Option<usize>,
    focused_widget: Option<usize>,
    click_count: u32,

    cursor_x: f32,
    cursor_y: f32,

    width: f32,
    height: f32,
    physical_width: u32,
    physical_height: u32,
    scale: f64,
    json_layout: Option<JsonLayoutWidget>,
    layout_mode: bool,
    ui_context: cce_ui::context::UiContext,
}

impl State {
    async fn new(
        wayland_handle: &'static cce_ui::wayland::WaylandSurfaceHandle,
        pw: u32,
        ph: u32,
        scale: f64,
        json_layout_config: Option<JsonLayoutConfig>,
    ) -> Self {
        cce_ui::scale::set_scale_factor(scale as f32);
        let lw = pw as f32 / scale as f32;
        let lh = ph as f32 / scale as f32;
        let sw = lw;
        let sh = lh;

        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::VULKAN,
            ..Default::default()
        });

        let surface = instance.create_surface(wayland_handle).unwrap();

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await
            .unwrap();

        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    label: None,
                    required_features: wgpu::Features::empty(),
                    required_limits: wgpu::Limits::default(),
                    memory_hints: wgpu::MemoryHints::MemoryUsage,
                },
                None,
            )
            .await
            .unwrap();

        let surface_caps = surface.get_capabilities(&adapter);
        let surface_format = surface_caps
            .formats
            .iter()
            .copied()
            .find(|f| f.is_srgb())
            .unwrap_or(surface_caps.formats[0]);
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: surface_format,
            width: pw,
            height: ph,
            present_mode: wgpu::PresentMode::Fifo,
            alpha_mode: surface_caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &config);

        let shader = device.create_shader_module(wgpu::include_wgsl!("shader.wgsl"));
        let render_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Render Pipeline Layout"),
            bind_group_layouts: &[],
            push_constant_ranges: &[],
        });
        let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Render Pipeline"),
            layout: Some(&render_pipeline_layout),
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
                    format: config.format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                unclipped_depth: false,
                polygon_mode: wgpu::PolygonMode::Fill,
                conservative: false,
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState {
                count: 1,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            multiview: None,
            cache: None,
        });

        let mut font_system = cce_ui::create_font_system();
        let swash_cache = SwashCache::new();
        let cache = Cache::new(&device);
        let viewport = Viewport::new(&device, &cache);
        let mut text_atlas = TextAtlas::new(&device, &queue, &cache, surface_format);
        let text_renderer =
            TextRenderer::new(&mut text_atlas, &device, wgpu::MultisampleState::default(), None);

        let label_buffer = make_text_buffer(&mut font_system, "Design System Playground", 16.0);
        let status_buffer = make_text_buffer(&mut font_system, "Ready", 12.0);

        let text_viewport = viewport;

        let layout_mode = json_layout_config.is_some();
        let mut widgets: Vec<Box<dyn Element>> = Vec::new();
        let mut positions = Vec::new();
        let json_layout = if let Some(ref config) = json_layout_config {
            let mut jl = JsonLayoutWidget::new(config);
            jl.set_rect(0.0, 0.0, sw, sh);
            positions.push((0.0, 0.0, sw, sh));
            Some(jl)
        } else {
            widgets = vec![
                Box::new(Header::new()),
                Box::new(Sidebar::new(60.0)),
                Box::new(ContentBg::new()),
                Box::new(Button::new(0.0, 0.0, 140.0, 40.0).with_label("Button A")),
                Box::new(Button::new(0.0, 0.0, 140.0, 40.0).with_label("Button B")),
                Box::new(Button::new(0.0, 0.0, 140.0, 40.0).with_label("Button C")),
                Box::new(Panel::new(0.0, 0.0, 400.0, 250.0)),
                Box::new(Button::new(0.0, 0.0, 140.0, 40.0).with_label("Click Me")),
                Box::new(Button::new_reset(0.0, 0.0, 140.0, 40.0).with_label("Reset")),
                Box::new(Checkbox::new()),
                Box::new(Toggle::new()),
                Box::new(ProgressBar::new(0.65)),
                Box::new(Slider::new()),
                Box::new(Spinbox::new(0, -10, 10, 1)),
                Box::new(RangeSlider::new()),
                Box::new(StatusBar::new()),
            ];
            positions = demo_positions(sw, sh);
            None
        };

        let vertex_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Vertex Buffer"),
            size: 1,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let mut state = Self {
            surface,
            device,
            queue,
            config,
            render_pipeline,
            vertex_buffer,
            vertex_count: 0,
            widgets,
            positions,
            font_system,
            swash_cache,
            text_atlas,
            text_renderer,
            text_viewport,
            label_buffer,
            status_buffer,
            drag_widget: None,
            focused_widget: None,
            click_count: 0,
            cursor_x: 0.0,
            cursor_y: 0.0,
            width: lw,
            height: lh,
            physical_width: pw,
            physical_height: ph,
            scale,
            json_layout,
            layout_mode,
            ui_context: cce_ui::context::UiContext::new(),
        };

        state.apply_layout();
        state.upload_vertices();
        state
    }

    fn apply_layout(&mut self) {
        if self.layout_mode {
            if let Some(jl) = &mut self.json_layout {
                cce_ui::scale::set_scale_factor(self.scale as f32);
                jl.set_rect(0.0, 0.0, self.width, self.height);
            }
        } else {
            for (i, pos) in self.positions.iter().enumerate() {
                if let Some(widget) = self.widgets.get_mut(i) {
                    if widget.is_dragging() {
                        continue;
                    }
                    let (x, y, w, h) = *pos;
                    widget.set_rect(x, y, w, h);
                }
            }
        }
    }

    fn collect_vertices(&self) -> Vec<Vertex> {
        let sw = self.width;
        let sh = self.height;
        let mut verts = Vec::new();
        if self.layout_mode {
            verts.extend(quad_vertices(0.0, 0.0, sw, sh, sw, sh, [0.05, 0.05, 0.08, 1.0]));
            if let Some(jl) = &self.json_layout {
                verts.extend(widget_vertices(jl, sw, sh));
                for (qx, qy, qw, qh, qc) in jl.extra_quads() {
                    verts.extend(extra_quad_vertices(jl, qx, qy, qw, qh, sw, sh, qc));
                }
                for (cx, cy, r, qc) in jl.extra_circles() {
                    verts.extend(circle_vertices(cx, cy, r, sw, sh, qc, 16));
                }
            }
        } else {
            let mut draw_order: Vec<usize> = (0..self.widgets.len()).collect();
            draw_order.sort_by_key(|&i| self.widgets[i].z_index());
            for &i in &draw_order {
                let w = &self.widgets[i];
                verts.extend(widget_vertices(w.as_ref(), sw, sh));
                for (qx, qy, qw, qh, qc) in w.extra_quads() {
                    verts.extend(extra_quad_vertices(w.as_ref(), qx, qy, qw, qh, sw, sh, qc));
                }
                for (cx, cy, r, qc) in w.extra_circles() {
                    verts.extend(circle_vertices(cx, cy, r, sw, sh, qc, 16));
                }
            }

            // Draw popover quads on top
            let mut popover_pc = cce_ui::layout::PopoverCollector::new();
            for &i in &draw_order {
                let w = &self.widgets[i];
                if w.popover_rect().is_some() {
                    w.render_popover(&mut popover_pc);
                }
            }
            for (qc, qx, qy, qw, qh) in popover_pc.rects {
                verts.extend(quad_vertices(qx, qy, qw, qh, sw, sh, qc));
            }

            if cce_ui::widget::context_menu::is_visible() {
                for (qx, qy, qw, qh, qc) in cce_ui::widget::context_menu::extra_quads() {
                    verts.extend(quad_vertices(qx, qy, qw, qh, sw, sh, qc));
                }
            }
        }
        verts
    }

    fn upload_vertices(&mut self) {
        let verts = self.collect_vertices();
        self.vertex_count = verts.len() as u32;
        if self.vertex_count == 0 {
            return;
        }
        let data = bytemuck::cast_slice(&verts);
        let needed = data.len() as wgpu::BufferAddress;
        if needed > self.vertex_buffer.size() {
            self.vertex_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("Vertex Buffer"),
                size: needed,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
        }
        self.queue.write_buffer(&self.vertex_buffer, 0, data);
    }

    fn update_status_text(&mut self, text: &str) {
        self.status_buffer = make_text_buffer(&mut self.font_system, text, 12.0);
    }

    fn prepare_text(&mut self) {
        let Self {
            ref mut text_renderer,
            ref device,
            ref queue,
            ref mut font_system,
            ref mut text_atlas,
            ref mut text_viewport,
            ref mut swash_cache,
            ref label_buffer,
            ref status_buffer,
            physical_width,
            physical_height,
            scale,
            ref json_layout,
            layout_mode,
            ref ui_context,
            ..
        } = self;

        let viewport = Resolution { width: *physical_width, height: *physical_height };
        text_viewport.update(queue, viewport);

        let scale_f32 = *scale as f32;

        let mut areas: Vec<TextArea> = Vec::new();
        if !*layout_mode {
            areas.push(TextArea {
                buffer: label_buffer,
                left: 80.0 * scale_f32,
                top: 12.0 * scale_f32,
                scale: scale_f32,
                bounds: TextBounds {
                    left: 0,
                    top: 0,
                    right: *physical_width as i32,
                    bottom: *physical_height as i32,
                },
                default_color: glyphon::Color::rgb(0xcc, 0xcc, 0xd4),
                custom_glyphs: &[],
            });
            areas.push(TextArea {
                buffer: status_buffer,
                left: 12.0 * scale_f32,
                top: *physical_height as f32 - 24.0 * scale_f32,
                scale: scale_f32,
                bounds: TextBounds {
                    left: 0,
                    top: 0,
                    right: *physical_width as i32,
                    bottom: *physical_height as i32,
                },
                default_color: glyphon::Color::rgb(0x55, 0x55, 0x66),
                custom_glyphs: &[],
            });
        }

        let mut widget_buffers: Vec<Buffer> = Vec::new();
        let mut widget_labels: Vec<(TextLabel, Option<[f32; 4]>)> = Vec::new();
        if *layout_mode {
            if let Some(jl) = json_layout {
                for (label, font, bounds) in jl.text_labels_with_font_and_bounds(ui_context) {
                    widget_buffers.push(make_text_buffer_with_font(font_system, &label.text, label.font_size, font.as_deref()));
                    widget_labels.push((label, bounds));
                }
            }
        } else {
            for (_i, w) in self.widgets.iter().enumerate() {
                for (label, font, bounds) in w.text_labels_with_font_and_bounds(ui_context) {
                    widget_buffers.push(make_text_buffer_with_font(font_system, &label.text, label.font_size, font.as_deref()));
                    widget_labels.push((label, bounds));
                }
            }

            if cce_ui::widget::context_menu::is_visible() {
                for label in cce_ui::widget::context_menu::text_labels() {
                    widget_buffers.push(make_text_buffer(font_system, &label.text, label.font_size));
                    widget_labels.push((label, None));
                }
            }
        }

        for (buf, (label, bounds)) in widget_buffers.iter().zip(widget_labels.iter()) {
            let item_bounds = if let Some([l, t, r, b]) = bounds {
                TextBounds {
                    left: (l * scale_f32).round() as i32,
                    top: (t * scale_f32).round() as i32,
                    right: (r * scale_f32).round() as i32,
                    bottom: (b * scale_f32).round() as i32,
                }
            } else {
                TextBounds {
                    left: 0,
                    top: 0,
                    right: *physical_width as i32,
                    bottom: *physical_height as i32,
                }
            };
            areas.push(TextArea {
                buffer: buf,
                left: label.x * scale_f32,
                top: label.y * scale_f32,
                scale: scale_f32,
                bounds: item_bounds,
                default_color: glyphon::Color::rgb(label.color[0], label.color[1], label.color[2]),
                custom_glyphs: &[],
            });
        }

        let mut popover_pc = cce_ui::layout::PopoverCollector::new();
        let mut popover_buffers = Vec::new();

        if !*layout_mode {
            // Draw popover texts on top
            for w in &self.widgets {
                if w.popover_rect().is_some() {
                    w.render_popover(&mut popover_pc);
                }
            }
            for (t, size, _x, _y, _tc, _font_opt, _bounds) in &popover_pc.texts {
                popover_buffers.push(make_text_buffer(font_system, t, *size));
            }
            for (buf, (_, _size, x, y, tc, _font_opt, bounds)) in popover_buffers.iter().zip(popover_pc.texts.iter()) {
                let item_bounds = if let Some([l, t, r, b]) = bounds {
                    TextBounds {
                        left: (l * scale_f32).round() as i32,
                        top: (t * scale_f32).round() as i32,
                        right: (r * scale_f32).round() as i32,
                        bottom: (b * scale_f32).round() as i32,
                    }
                } else {
                    TextBounds {
                        left: 0,
                        top: 0,
                        right: *physical_width as i32,
                        bottom: *physical_height as i32,
                    }
                };
                areas.push(TextArea {
                    buffer: buf,
                    left: *x * scale_f32,
                    top: *y * scale_f32,
                    scale: scale_f32,
                    bounds: item_bounds,
                default_color: glyphon::Color::rgb(
                    (tc[0] * 255.0) as u8,
                    (tc[1] * 255.0) as u8,
                    (tc[2] * 255.0) as u8,
                ),
                custom_glyphs: &[],
            });
        }
        }

        text_renderer
            .prepare(device, queue, font_system, text_atlas, text_viewport, areas, swash_cache)
            .unwrap();
    }

    fn resize(&mut self, width: u32, height: u32) {
        if width > 0 && height > 0 {
            self.physical_width = width;
            self.physical_height = height;
            self.width = width as f32 / self.scale as f32;
            self.height = height as f32 / self.scale as f32;
            self.config.width = width;
            self.config.height = height;
            self.surface.configure(&self.device, &self.config);
            if self.layout_mode {
                self.positions = vec![(0.0, 0.0, self.width, self.height)];
            } else {
                self.positions = demo_positions(self.width, self.height);
            }
            self.apply_layout();
            self.upload_vertices();
        }
    }

    fn render(&mut self) {
        self.prepare_text();

        let output = match self.surface.get_current_texture() {
            Ok(t) => t,
            Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                self.surface.configure(&self.device, &self.config);
                match self.surface.get_current_texture() {
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

        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Encoder"),
        });

        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Render Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.05,
                            g: 0.05,
                            b: 0.08,
                            a: 0.20,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            if self.vertex_count > 0 {
                pass.set_pipeline(&self.render_pipeline);
                pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
                pass.draw(0..self.vertex_count, 0..1);
            }

            self.text_renderer
                .render(&self.text_atlas, &self.text_viewport, &mut pass)
                .unwrap();
        }

        self.queue.submit(std::iter::once(encoder.finish()));
        output.present();
    }
}

fn demo_positions(sw: f32, sh: f32) -> Vec<(f32, f32, f32, f32)> {
    vec![
        (0.0, 0.0, sw, 40.0),               // 0 header
        (0.0, 40.0, 60.0, sh - 68.0),       // 1 sidebar
        (60.0, 40.0, sw - 60.0, sh - 68.0), // 2 content_bg
        (70.0, 50.0, 140.0, 40.0),          // 3 btn_a
        (220.0, 50.0, 140.0, 40.0),         // 4 btn_b
        (370.0, 50.0, 140.0, 40.0),         // 5 btn_c
        (70.0, 100.0, 400.0, 250.0),        // 6 panel
        (70.0, 360.0, 140.0, 40.0),         // 7 click_me
        (220.0, 360.0, 140.0, 40.0),        // 8 reset
        (70.0, 410.0, 24.0, 24.0),          // 9 checkbox
        (104.0, 410.0, 48.0, 24.0),         // 10 toggle
        (162.0, 410.0, 160.0, 24.0),        // 11 progress_bar
        (70.0, 444.0, 300.0, 32.0),         // 12 slider
        (70.0, 486.0, 120.0, 32.0),         // 13 spinbox
        (70.0, 528.0, 300.0, 32.0),         // 14 range_slider
        (0.0, sh - 28.0, sw, 28.0),         // 15 status_bar
    ]
}

struct PressedKey {
    logical_key: cce_ui::widget::Key,
    text: Option<String>,
    first_pressed: std::time::Instant,
    last_repeated: std::time::Instant,
}

fn is_repeatable_key(key: &cce_ui::widget::Key) -> bool {
    use cce_ui::widget::{Key, NamedKey};
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

struct AppState {
    registry_state: RegistryState,
    compositor_state: CompositorState,
    xdg_shell_state: XdgShell,
    shm_state: Shm,
    seat_state: SeatState,
    output_state: OutputState,

    seats: Vec<wl_seat::WlSeat>,
    pointer: Option<ThemedPointer>,
    keyboard: Option<wl_keyboard::WlKeyboard>,

    window: Option<XdgWindow>,
    surface: Option<wl_surface::WlSurface>,

    state: Option<State>,
    exit: bool,
    redraw: bool,
    ctrl_pressed: bool,
    shift_pressed: bool,
    pressed_key: Option<PressedKey>,
    key_repeat_delay: std::time::Duration,
    key_repeat_interval: std::time::Duration,
    inspector: Option<cce_ui::protocol::zcce_inspector_v1::ZcceInspectorV1>,
    last_inspector_update: std::time::Instant,
    last_serialized: String,
}

impl CompositorHandler for AppState {
    fn scale_factor_changed(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        scale_factor: i32,
    ) {
        _surface.set_buffer_scale(scale_factor);
        if let Some(state) = &mut self.state {
            state.scale = scale_factor as f64;
            let pw = (state.width as f64 * state.scale) as u32;
            let ph = (state.height as f64 * state.scale) as u32;
            state.resize(pw, ph);
        }
        self.redraw = true;
    }

    fn transform_changed(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        _new_transform: wl_output::Transform,
    ) {
    }

    fn frame(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        _time: u32,
    ) {
    }

    fn surface_enter(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        _output: &wl_output::WlOutput,
    ) {
    }

    fn surface_leave(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        _output: &wl_output::WlOutput,
    ) {
    }
}

impl OutputHandler for AppState {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.output_state
    }

    fn new_output(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, _output: wl_output::WlOutput) {}

    fn update_output(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, _output: wl_output::WlOutput) {}

    fn output_destroyed(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, _output: wl_output::WlOutput) {}
}

impl SeatHandler for AppState {
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

impl ShmHandler for AppState {
    fn shm_state(&mut self) -> &mut Shm {
        &mut self.shm_state
    }
}

impl PointerHandler for AppState {
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
        for event in events {
            let (x, y) = event.position;
            if let Some(state) = &mut self.state {
                state.cursor_x = x as f32;
                state.cursor_y = y as f32;
            }

            match &event.kind {
                PointerEventKind::Enter { .. } => {
                    if let Some(ref themed_pointer) = self.pointer {
                        let _ = themed_pointer.set_cursor(_conn, CursorIcon::Default);
                    }
                }
                PointerEventKind::Leave { .. } => {}
                PointerEventKind::Motion { .. } => {
                    if let Some(state) = &mut self.state {
                        let mut changed = false;
                        if cce_ui::widget::context_menu::is_visible() {
                            if cce_ui::widget::context_menu::cursor_moved(state.cursor_x, state.cursor_y) {
                                changed = true;
                            }
                        } else if state.layout_mode {
                            if let Some(jl) = &mut state.json_layout {
                                if jl.cursor_moved(state.cursor_x, state.cursor_y, &mut state.ui_context) {
                                    changed = true;
                                }
                            }
                        } else {
                            if let Some(idx) = state.drag_widget {
                                if state.widgets[idx].drag_update(state.cursor_x, state.cursor_y) {
                                    changed = true;
                                }
                            }
                            if state.drag_widget.is_none() {
                                for w in &mut state.widgets {
                                    if w.cursor_moved(state.cursor_x, state.cursor_y, &mut state.ui_context) {
                                        changed = true;
                                    }
                                }
                            }
                        }
                        if changed {
                            state.upload_vertices();
                            self.redraw = true;
                        }
                    }
                }
                PointerEventKind::Press { button, .. } => {
                    let btn = match *button {
                        272 => cce_ui::widget::MouseButton::Left,
                        273 => cce_ui::widget::MouseButton::Right,
                        274 => cce_ui::widget::MouseButton::Middle,
                        _ => continue,
                    };
                    if let Some(st) = &mut self.state {
                        let mut changed = false;
                        if cce_ui::widget::context_menu::is_visible() {
                            if cce_ui::widget::context_menu::mouse_input(btn, cce_ui::widget::ElementState::Pressed, st.cursor_x, st.cursor_y) {
                                changed = true;
                            }
                        } else if st.layout_mode {
                            if let Some(jl) = &mut st.json_layout {
                                if jl.mouse_input(btn, cce_ui::widget::ElementState::Pressed, st.cursor_x, st.cursor_y, &mut st.ui_context) {
                                    changed = true;
                                }
                            }
                        } else {
                            let mut clicked_idx = None;
                            for i in (0..st.widgets.len()).rev() {
                                if st.widgets[i].hit_test(st.cursor_x, st.cursor_y, &st.ui_context) {
                                    clicked_idx = Some(i);
                                    break;
                                }
                            }
                            if btn == cce_ui::widget::MouseButton::Left {
                                if let Some(old) = st.focused_widget {
                                    if Some(old) != clicked_idx {
                                        st.widgets[old].unfocus();
                                        st.focused_widget = None;
                                    }
                                }
                            }
                            if let Some(i) = clicked_idx {
                                if st.widgets[i].mouse_input(
                                    btn,
                                    cce_ui::widget::ElementState::Pressed,
                                    st.cursor_x,
                                    st.cursor_y,
                                    &mut st.ui_context,
                                ) {
                                    changed = true;
                                }
                                if btn == cce_ui::widget::MouseButton::Left && st.widgets[i].draggable() {
                                    st.widgets[i].drag_begin(st.cursor_x, st.cursor_y);
                                    st.drag_widget = Some(i);
                                }
                                if btn == cce_ui::widget::MouseButton::Left {
                                    st.widgets[i].focus();
                                    st.focused_widget = Some(i);
                                }
                            }
                        }
                        if changed {
                            st.upload_vertices();
                            self.redraw = true;
                        }
                    }
                }
                PointerEventKind::Release { button, .. } => {
                    let btn = match *button {
                        272 => cce_ui::widget::MouseButton::Left,
                        273 => cce_ui::widget::MouseButton::Right,
                        274 => cce_ui::widget::MouseButton::Middle,
                        _ => continue,
                    };
                    if let Some(st) = &mut self.state {
                        let mut changed = false;
                        let mut should_close = false;
                        if cce_ui::widget::context_menu::is_visible() {
                            if cce_ui::widget::context_menu::mouse_input(btn, cce_ui::widget::ElementState::Released, st.cursor_x, st.cursor_y) {
                                changed = true;
                            }
                        } else if st.layout_mode {
                            let mut clicked_btn_id = None;
                            if let Some(jl) = &mut st.json_layout {
                                if jl.mouse_input(btn, cce_ui::widget::ElementState::Released, st.cursor_x, st.cursor_y, &mut st.ui_context) {
                                    changed = true;
                                }
                                if btn == cce_ui::widget::MouseButton::Left {
                                    for w in &mut jl.widgets {
                                        let active_page = 0;
                                        if w.page_idx != active_page {
                                            continue;
                                        }
                                        // take_click is an Element method; Phase 5 Buttons
                                        // are Adapted, so ask the box directly.
                                        if w.widget_type == "button" && w.widget.take_click() {
                                            clicked_btn_id = Some(w.id.clone());
                                            break;
                                        }
                                    }
                                }
                            }
                            if let Some(btn_id) = clicked_btn_id {
                                let mut checkboxes = std::collections::HashMap::new();
                                let mut spinboxes = std::collections::HashMap::new();
                                let mut colors = std::collections::HashMap::new();
                                let mut sliders = std::collections::HashMap::new();
                                if let Some(jl) = &st.json_layout {
                                    for w in &jl.widgets {
                                        if let Some(cb) = w.widget.as_any().downcast_ref::<cce_ui::widget::Checkbox>() {
                                            checkboxes.insert(w.id.clone(), cb.checked());
                                        } else if let Some(sb) = w.widget.as_any().downcast_ref::<cce_ui::widget::Spinbox>() {
                                            spinboxes.insert(w.id.clone(), sb.value);
                                        } else if let Some(cs) = w.widget.as_any().downcast_ref::<cce_ui::widget::ColorSelector>() {
                                            colors.insert(w.id.clone(), cs.color);
                                        } else if let Some(sl) = w.widget.as_any().downcast_ref::<cce_ui::widget::Slider>() {
                                            sliders.insert(w.id.clone(), sl.get_scaled_value());
                                        }
                                    }
                                }
                                let out_val = serde_json::json!({
                                    "button": btn_id,
                                    "checkboxes": checkboxes,
                                    "spinboxes": spinboxes,
                                    "colors": colors,
                                    "sliders": sliders
                                });
                                println!("{}", out_val.to_string());
                                should_close = true;
                            }
                        } else {
                            if btn == cce_ui::widget::MouseButton::Left {
                                if let Some(idx) = st.drag_widget {
                                    st.widgets[idx].drag_end();
                                    st.drag_widget = None;
                                    changed = true;
                                }
                            }
                            for w in &mut st.widgets {
                                if w.mouse_input(btn, cce_ui::widget::ElementState::Released, st.cursor_x, st.cursor_y, &mut st.ui_context) {
                                    changed = true;
                                }
                            }
                            if btn == cce_ui::widget::MouseButton::Left {
                                let mut clicked = false;
                                for w in &mut st.widgets {
                                    if w.take_click() {
                                        clicked = true;
                                    }
                                }
                                if clicked {
                                    st.click_count += 1;
                                    st.update_status_text(&format!("Clicks: {}", st.click_count));
                                }
                            }
                        }
                        if changed {
                            st.upload_vertices();
                            self.redraw = true;
                        }
                        if should_close {
                            self.exit = true;
                        }
                    }
                }
                PointerEventKind::Axis { horizontal, vertical, .. } => {
                    coalesced_h += horizontal.absolute;
                    coalesced_v += vertical.absolute;
                    discrete_h += horizontal.discrete;
                    discrete_v += vertical.discrete;
                    has_scroll = true;
                }
            }
        }

        if has_scroll {
            if let Some(state) = &mut self.state {
                let delta = if discrete_h == 0 && discrete_v == 0 {
                    cce_ui::widget::MouseScrollDelta::PixelDelta(cce_ui::widget::Position {
                        x: -coalesced_h,
                        y: -coalesced_v,
                    })
                } else {
                    let h_lines = if discrete_h != 0 { discrete_h as f32 } else { coalesced_h as f32 / 10.0 };
                    let v_lines = if discrete_v != 0 { discrete_v as f32 } else { coalesced_v as f32 / 10.0 };
                    cce_ui::widget::MouseScrollDelta::LineDelta(-h_lines, -v_lines)
                };

                let mut changed = false;
                if !state.layout_mode {
                    for w in &mut state.widgets {
                        if w.mouse_wheel(&delta, state.cursor_x, state.cursor_y, &mut state.ui_context) {
                            changed = true;
                        }
                    }
                } else {
                    if let Some(jl) = &mut state.json_layout {
                        if jl.mouse_wheel(&delta, state.cursor_x, state.cursor_y, &mut state.ui_context) {
                            changed = true;
                        }
                    }
                }
                if changed {
                    state.upload_vertices();
                    self.redraw = true;
                }
            }
        }
    }
}

impl KeyboardHandler for AppState {
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
    }

    fn press_key(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _keyboard: &wl_keyboard::WlKeyboard,
        _serial: u32,
        event: smithay_client_toolkit::seat::keyboard::KeyEvent,
    ) {
        self.handle_key(event, cce_ui::widget::ElementState::Pressed);
    }

    fn release_key(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _keyboard: &wl_keyboard::WlKeyboard,
        _serial: u32,
        event: smithay_client_toolkit::seat::keyboard::KeyEvent,
    ) {
        self.handle_key(event, cce_ui::widget::ElementState::Released);
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
                self.key_repeat_delay = std::time::Duration::from_millis(delay as u64);
                let interval_ms = 1000 / rate.get() as u64;
                self.key_repeat_interval = std::time::Duration::from_millis(interval_ms);
            }
            smithay_client_toolkit::seat::keyboard::RepeatInfo::Disable => {
                self.key_repeat_delay = std::time::Duration::from_secs(999999);
            }
        }
    }
}

impl AppState {
    fn handle_key(&mut self, event: smithay_client_toolkit::seat::keyboard::KeyEvent, state: cce_ui::widget::ElementState) {
        use cce_ui::widget::{Key, KeyEvent, NamedKey};
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

        if cce_ui::widget::context_menu::is_visible() {
            if custom_event.state == cce_ui::widget::ElementState::Pressed
                && custom_event.logical_key == Key::Named(NamedKey::Escape)
            {
                cce_ui::widget::context_menu::hide();
                if let Some(st) = &mut self.state {
                    st.upload_vertices();
                }
                self.redraw = true;
                return;
            }
        }

        if state == cce_ui::widget::ElementState::Pressed {
            if is_repeatable_key(&custom_event.logical_key) {
                self.pressed_key = Some(PressedKey {
                    logical_key: custom_event.logical_key.clone(),
                    text: custom_event.text.clone(),
                    first_pressed: std::time::Instant::now(),
                    last_repeated: std::time::Instant::now(),
                });
            } else {
                self.pressed_key = None;
            }
        } else if state == cce_ui::widget::ElementState::Released {
            if let Some(ref pk) = self.pressed_key {
                if pk.logical_key == custom_event.logical_key {
                    self.pressed_key = None;
                }
            }
        }

        if let Some(st) = &mut self.state {
            if st.layout_mode {
                if let Some(jl) = &mut st.json_layout {
                    let changed = jl.keyboard_input(&custom_event, &mut st.ui_context);
                    if changed {
                        st.upload_vertices();
                        self.redraw = true;
                    }
                }
            } else if let Some(idx) = st.focused_widget {
                let val = st.widgets[idx].value();
                let mut changed = st.widgets[idx].keyboard_input(&custom_event, &mut st.ui_context);
                if st.widgets[idx].value() != val {
                    changed = true;
                }
                if changed {
                    st.upload_vertices();
                    self.redraw = true;
                }
            }
        }
    }
}

impl WindowHandler for AppState {
    fn configure(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _window: &XdgWindow,
        configure: WindowConfigure,
        _serial: u32,
    ) {
        let (w, h) = configure.new_size;
        if let (Some(w), Some(h)) = (w, h) {
            let width = w.get();
            let height = h.get();
            if let Some(state) = &mut self.state {
                let pw = (width as f64 * state.scale) as u32;
                let ph = (height as f64 * state.scale) as u32;
                state.resize(pw, ph);
            }
        }
        self.redraw = true;
    }

    fn request_close(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, _window: &XdgWindow) {
        self.exit = true;
    }
}

impl ProvidesRegistryState for AppState {
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

impl wayland_client::Dispatch<cce_ui::protocol::zcce_inspector_v1::ZcceInspectorV1, ()> for AppState {
    fn event(
        _state: &mut Self,
        _proxy: &cce_ui::protocol::zcce_inspector_v1::ZcceInspectorV1,
        _event: cce_ui::protocol::zcce_inspector_v1::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {}
}

delegate_compositor!(AppState);
delegate_xdg_shell!(AppState);
delegate_xdg_window!(AppState);
delegate_shm!(AppState);
delegate_seat!(AppState);
delegate_pointer!(AppState);
delegate_keyboard!(AppState);
delegate_registry!(AppState);
delegate_output!(AppState);

fn main() {
    let mut layout_mode = false;
    let mut json_layout_config: Option<JsonLayoutConfig> = None;

    let args = std::env::args().skip(1).collect::<Vec<String>>();
    let mut idx = 0;
    while idx < args.len() {
        let arg = &args[idx];
        if arg == "--layout" || arg == "--json" {
            layout_mode = true;
            idx += 1;
        } else {
            idx += 1;
        }
    }

    if layout_mode {
        use std::io::Read;
        let mut json_str = String::new();
        let mut stdin = std::io::stdin();
        match stdin.read_to_string(&mut json_str) {
            Ok(_) => {
                match serde_json::from_str::<JsonLayoutConfig>(&json_str) {
                    Ok(cfg) => {
                        json_layout_config = Some(cfg);
                    }
                    Err(e) => {
                        log::error!("Failed to parse JSON layout: {}", e);
                        std::process::exit(1);
                    }
                }
            }
            Err(e) => {
                log::error!("Failed to read JSON layout from stdin: {}", e);
                std::process::exit(1);
            }
        }
    }

    let conn = Connection::connect_to_env().unwrap();
    let (globals, mut event_queue) = registry_queue_init(&conn).unwrap();
    let qh = event_queue.handle();

    let compositor_state = CompositorState::bind(&globals, &qh).unwrap();
    let xdg_shell_state = XdgShell::bind(&globals, &qh).unwrap();
    let shm_state = Shm::bind(&globals, &qh).unwrap();
    let seat_state = SeatState::new(&globals, &qh);
    let output_state = OutputState::new(&globals, &qh);
    let inspector = globals.bind(&qh, 1..=1, ()).ok();

    let mut app = AppState {
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
        state: None,
        exit: false,
        redraw: true,
        ctrl_pressed: false,
        shift_pressed: false,
        pressed_key: None,
        key_repeat_delay: std::time::Duration::from_millis(500),
        key_repeat_interval: std::time::Duration::from_millis(50),
        inspector,
        last_inspector_update: std::time::Instant::now() - std::time::Duration::from_secs(1),
        last_serialized: String::new(),
    };

    // Perform a roundtrip to populate output_state with active output scales
    event_queue.roundtrip(&mut app).unwrap();

    let scale = cce_ui::wayland::detect_scale_factor(&app.output_state);

    let surface = app.compositor_state.create_surface(&qh);
    surface.set_buffer_scale(scale as i32);

    let mut win_w = 1024.0;
    let mut win_h = 768.0;
    if let Some(ref cfg) = json_layout_config {
        if let Some(w) = cfg.width {
            win_w = w as f64;
        }
        if let Some(h) = cfg.height {
            win_h = h as f64;
        }
    }
    let pw = (win_w * scale) as u32;
    let ph = (win_h * scale) as u32;

    let window = app.xdg_shell_state.create_window(surface.clone(), WindowDecorations::None, &qh);
    window.set_title("CCE UI - Test Window");
    window.set_app_id("cce-ui");
    window.set_min_size(Some((win_w as u32, win_h as u32)));
    window.commit();

    if let Some(ref inspector) = app.inspector {
        inspector.register_client(&surface);
    }

    let wayland_handle = Box::leak(Box::new(cce_ui::wayland::WaylandSurfaceHandle {
        display_ptr: conn.backend().display_id().as_ptr() as *mut std::ffi::c_void,
        surface_ptr: surface.id().as_ptr() as *mut std::ffi::c_void,
    }));

    let state = pollster::block_on(State::new(wayland_handle, pw, ph, scale, json_layout_config));

    app.window = Some(window);
    app.surface = Some(surface);
    app.state = Some(state);

    let mut event_loop = EventLoop::try_new().unwrap();
    let loop_handle = event_loop.handle();
    WaylandSource::new(conn, event_queue).insert(loop_handle).unwrap();



    let mut last_tick = std::time::Instant::now();

    loop {
        event_loop
            .dispatch(std::time::Duration::from_millis(16), &mut app)
            .unwrap();
        if app.exit {
            break;
        }

        let now = std::time::Instant::now();
        let dt = now.duration_since(last_tick).as_secs_f32();
        last_tick = now;

        if let Some(ref mut st) = app.state {
            let tick_changed = st.ui_context.tick(dt);
            if tick_changed {
                st.upload_vertices();
                app.redraw = true;
            }
        }

        if let Some(ref mut pk) = app.pressed_key {
            let now = std::time::Instant::now();
            if now.duration_since(pk.first_pressed) >= app.key_repeat_delay {
                if now.duration_since(pk.last_repeated) >= app.key_repeat_interval {
                    pk.last_repeated = now;
                    let custom_event = cce_ui::widget::KeyEvent {
                        state: cce_ui::widget::ElementState::Pressed,
                        logical_key: pk.logical_key.clone(),
                        text: pk.text.clone(),
                        repeat: true,
                        ctrl: app.ctrl_pressed,
                        shift: app.shift_pressed,
                    };
                    if let Some(st) = &mut app.state {
                        if st.layout_mode {
                            if let Some(jl) = &mut st.json_layout {
                                let changed = jl.keyboard_input(&custom_event, &mut st.ui_context);
                                if changed {
                                    st.upload_vertices();
                                    app.redraw = true;
                                }
                            }
                        } else if let Some(idx) = st.focused_widget {
                            let val = st.widgets[idx].value();
                            let mut changed = st.widgets[idx].keyboard_input(&custom_event, &mut st.ui_context);
                            if st.widgets[idx].value() != val {
                                changed = true;
                            }
                            if changed {
                                st.upload_vertices();
                                app.redraw = true;
                            }
                        }
                    }
                }
            }
        }

fn create_memfd_with_data(name: &str, data: &[u8]) -> std::io::Result<std::os::unix::io::RawFd> {
    use std::io::{Seek, Write};
    use std::os::unix::io::FromRawFd;
    use std::os::unix::io::IntoRawFd;

    let c_name = std::ffi::CString::new(name).unwrap();
    let fd = unsafe { libc::memfd_create(c_name.as_ptr(), libc::MFD_CLOEXEC) };
    if fd < 0 {
        return Err(std::io::Error::last_os_error());
    }
    let mut file = unsafe { std::fs::File::from_raw_fd(fd) };
    file.write_all(data)?;
    file.seek(std::io::SeekFrom::Start(0))?;
    Ok(file.into_raw_fd())
}

        if app.redraw {
            app.redraw = false;
            if let Some(state) = &mut app.state {
                state.render();
                if let Some(ref inspector) = app.inspector {
                    if let Some(ref surface) = app.surface {
                        let json = cce_ui::widget::serialize_widgets(&state.widgets);
                        if json != app.last_serialized {
                            let now = std::time::Instant::now();
                            if now.duration_since(app.last_inspector_update) >= std::time::Duration::from_millis(100) {
                                app.last_serialized = json.clone();
                                app.last_inspector_update = now;
                                if let Ok(raw_fd) = create_memfd_with_data("cce_ui_state", json.as_bytes()) {
                                    use std::os::unix::io::{FromRawFd, AsFd};
                                    let file = unsafe { std::fs::File::from_raw_fd(raw_fd) };
                                    inspector.update_state(surface, file.as_fd(), json.len() as u32);
                                }
                            } else {
                                app.redraw = true;
                            }
                        }
                    }
                }
            }
        }
    }
}
