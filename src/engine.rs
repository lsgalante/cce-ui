use std::time::Instant;
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
    globals::{registry_queue_init, GlobalList},
    protocol::{wl_keyboard, wl_output, wl_pointer, wl_seat, wl_shm, wl_surface, wl_registry},
    Connection, QueueHandle, Proxy,
};
use calloop::EventLoop;
use calloop_wayland_source::WaylandSource;
use glyphon::{
    Cache, FontSystem, Resolution, SwashCache, TextArea, TextAtlas,
    TextBounds, TextRenderer, Viewport,
};
use crate::widget::{TextItem, MouseButton, ElementState, MouseScrollDelta, KeyEvent, Key, NamedKey};
use crate::wayland::{WaylandSurfaceHandle, detect_scale_factor};

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

pub fn widget_vertices(w: &dyn crate::widget::Widget, sw: f32, sh: f32, clip_circle: [f32; 3]) -> Vec<Vertex> {
    let (x, y, ww, h) = w.rect();
    quad_vertices_with_clip(x, y, ww, h, sw, sh, w.color(), clip_circle).to_vec()
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
        
        verts.push(Vertex { position: [ndc_x0, ndc_y0], color, clip_circle });
        verts.push(Vertex { position: [ndc_x1, ndc_y1], color, clip_circle });
        verts.push(Vertex { position: [ndc_x2, ndc_y2], color, clip_circle });
        
        verts.push(Vertex { position: [ndc_x0, ndc_y0], color, clip_circle });
        verts.push(Vertex { position: [ndc_x2, ndc_y2], color, clip_circle });
        verts.push(Vertex { position: [ndc_x3, ndc_y3], color, clip_circle });
    }
    verts
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
    fn overlay_quads(&mut self, _quads: &mut Vec<(f32, f32, f32, f32, [f32; 4])>, _size: LogicalSize, _scale: f64) {}
    fn text_items(&self) -> &[TextItem];
    
    fn text_areas(&self, scale_f32: f32, bounds: TextBounds) -> Vec<TextArea<'_>> {
        self.text_items().iter().map(|ti| TextArea {
            buffer: &ti.buffer,
            left: ti.x * scale_f32,
            top: ti.y * scale_f32,
            scale: scale_f32,
            bounds,
            default_color: ti.color,
            custom_glyphs: &[],
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
    
    pub wgpu_surface: Option<wgpu::Surface<'static>>,
    pub device: Option<wgpu::Device>,
    pub queue: Option<wgpu::Queue>,
    pub config: Option<wgpu::SurfaceConfiguration>,
    pub render_pipeline: Option<wgpu::RenderPipeline>,
    pub vertex_buffer: Option<wgpu::Buffer>,
    pub vertex_count: u32,
    pub overlay_vertex_buffer: Option<wgpu::Buffer>,
    pub overlay_vertex_count: u32,
    
    pub font_system: FontSystem,
    pub swash_cache: SwashCache,
    pub text_atlas: Option<TextAtlas>,
    pub text_renderer: Option<TextRenderer>,
    pub text_viewport: Option<Viewport>,
    
    pub scale_factor: f64,
    pub logical_width: f32,
    pub logical_height: f32,
    
    pub exit: bool,
    pub redraw: bool,
    pub first_configure_received: bool,
    pub ctrl_pressed: bool,
    pub shift_pressed: bool,
    pub pressed_key: Option<PressedKey>,
    pub sender: calloop::channel::Sender<A::Message>,
}

impl<A: Application> EngineState<A> {
    pub async fn init_gpu(&mut self, conn: &Connection, width_logical: f32, height_logical: f32) {
        let s = self.scale_factor as f32;
        let pw = (width_logical * s) as u32;
        let ph = (height_logical * s) as u32;
        
        let surface = self.surface.as_ref().expect("surface missing");
        
        let wayland_handle = Box::leak(Box::new(WaylandSurfaceHandle {
            display_ptr: conn.backend().display_id().as_ptr() as *mut std::ffi::c_void,
            surface_ptr: surface.id().as_ptr() as *mut std::ffi::c_void,
        }));
        
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::VULKAN,
            ..Default::default()
        });
        
        let wgpu_surface = instance.create_surface(wayland_handle).expect("failed to create wgpu surface");
        let adapter = instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::LowPower,
            compatible_surface: Some(&wgpu_surface),
            force_fallback_adapter: false,
        }).await.expect("failed to request adapter");
        
        let (device, queue) = adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("GPU Device"),
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits::downlevel_webgl2_defaults().using_resolution(adapter.limits()),
            memory_hints: wgpu::MemoryHints::MemoryUsage,
        }, None).await.expect("failed to request device");
        
        let config = wgpu_surface.get_default_config(&adapter, pw.max(1), ph.max(1)).expect("failed to get surface configuration");
        
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Shader"),
            source: wgpu::ShaderSource::Wgsl(crate::SHADER.into()),
        });
        
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Pipeline Layout"),
            bind_group_layouts: &[],
            push_constant_ranges: &[],
        });
        
        let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
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
                    format: config.format,
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
        
        let cache = Cache::new(&device);
        let mut text_atlas = TextAtlas::new(&device, &queue, &cache, config.format);
        let text_renderer = TextRenderer::new(&mut text_atlas, &device, wgpu::MultisampleState::default(), None);
        let mut text_viewport = Viewport::new(&device, &cache);
        text_viewport.update(&queue, Resolution { width: pw, height: ph });
        
        let vertex_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Vertex Buffer"),
            size: 1,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let overlay_vertex_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Overlay Vertex Buffer"),
            size: 1,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        
        self.wgpu_surface = Some(wgpu_surface);
        self.device = Some(device);
        self.queue = Some(queue);
        self.config = Some(config);
        self.render_pipeline = Some(render_pipeline);
        self.text_atlas = Some(text_atlas);
        self.text_renderer = Some(text_renderer);
        self.text_viewport = Some(text_viewport);
        self.vertex_buffer = Some(vertex_buffer);
        self.overlay_vertex_buffer = Some(overlay_vertex_buffer);
        self.logical_width = width_logical;
        self.logical_height = height_logical;
    }
    
    pub fn resize(&mut self, w: f32, h: f32) {
        if w > 0.0 && h > 0.0 {
            self.logical_width = w;
            self.logical_height = h;
            if let (Some(device), Some(surface), Some(config)) = (&self.device, &self.wgpu_surface, &mut self.config) {
                config.width = (w as f64 * self.scale_factor) as u32;
                config.height = (h as f64 * self.scale_factor) as u32;
                surface.configure(device, config);
            }
        }
    }
    
    pub fn render(&mut self) {
        let logical_w = self.logical_width;
        let logical_h = self.logical_height;
        let scale_factor = self.scale_factor;
        
        let mut quads = Vec::new();
        self.inner.view(&mut quads, LogicalSize::new(logical_w, logical_h), scale_factor);
        
        let device = self.device.as_ref().unwrap();
        let queue = self.queue.as_ref().unwrap();
        let wgpu_surface = self.wgpu_surface.as_ref().unwrap();
        let render_pipeline = self.render_pipeline.as_ref().unwrap();
        let text_renderer = self.text_renderer.as_mut().unwrap();
        let text_atlas = self.text_atlas.as_mut().unwrap();
        let text_viewport = self.text_viewport.as_mut().unwrap();
        let config = self.config.as_ref().unwrap();
        
        // 1. Build and upload vertex buffer
        let mut verts = Vec::new();
        for &(qx, qy, qw, qh, qc) in &quads {
            verts.extend(quad_vertices(qx, qy, qw, qh, logical_w, logical_h, qc));
        }
        self.vertex_count = verts.len() as u32;
        if self.vertex_count > 0 {
            let data = bytemuck::cast_slice(&verts);
            let needed = data.len() as wgpu::BufferAddress;
            let mut vbuf = self.vertex_buffer.as_ref().unwrap();
            if needed > vbuf.size() {
                let new_vbuf = device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("Vertex Buffer"),
                    size: needed,
                    usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                });
                self.vertex_buffer = Some(new_vbuf);
                vbuf = self.vertex_buffer.as_ref().unwrap();
            }
            queue.write_buffer(vbuf, 0, data);
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
                let new_ovbuf = device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("Overlay Vertex Buffer"),
                    size: needed,
                    usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                });
                self.overlay_vertex_buffer = Some(new_ovbuf);
                ovbuf = self.overlay_vertex_buffer.as_ref().unwrap();
            }
            queue.write_buffer(ovbuf, 0, data);
        }
        
        // 2. Prepare text
        let scale_f32 = scale_factor as f32;
        let pw = (logical_w * scale_f32) as u32;
        let ph = (logical_h * scale_f32) as u32;
        text_viewport.update(queue, Resolution { width: pw, height: ph });
        
        let bounds = TextBounds { left: 0, top: 0, right: pw as i32, bottom: ph as i32 };
        let areas = self.inner.text_areas(scale_f32, bounds);
        
        text_renderer.prepare(device, queue, &mut self.font_system, text_atlas, text_viewport, areas, &mut self.swash_cache).unwrap();
        
        // 3. Render Pass
        let output = match wgpu_surface.get_current_texture() {
            Ok(t) => t,
            Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                wgpu_surface.configure(device, config);
                return;
            }
            Err(wgpu::SurfaceError::Timeout) => return,
            Err(e) => {
                eprintln!("Surface error: {e:?}");
                return;
            }
        };
        let view = output.texture.create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
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
            
            text_renderer.render(text_atlas, text_viewport, &mut pass).unwrap();

            if self.overlay_vertex_count > 0 {
                pass.set_pipeline(render_pipeline);
                pass.set_vertex_buffer(0, self.overlay_vertex_buffer.as_ref().unwrap().slice(..));
                pass.draw(0..self.overlay_vertex_count, 0..1);
            }
        }
        
        queue.submit(std::iter::once(encoder.finish()));
        output.present();
        text_atlas.trim();
    }
}

impl<A: Application> Drop for EngineState<A> {
    fn drop(&mut self) {
        self.wgpu_surface = None;
        self.device = None;
        self.queue = None;
        self.render_pipeline = None;
        self.vertex_buffer = None;
        self.overlay_vertex_buffer = None;
        self.text_atlas = None;
        self.text_renderer = None;
        self.text_viewport = None;
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
    
    fn new_output(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, _output: wl_output::WlOutput) {}
    fn update_output(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, _output: wl_output::WlOutput) {}
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
        let (w, h) = configure.new_size;
        if let (Some(w), Some(h)) = (w, h) {
            let width = w.get();
            let height = h.get();
            self.resize(width as f32, height as f32);
        }
        self.redraw = true;
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
        for event in events {
            let (x, y) = event.position;
            let lx = x as f32;
            let ly = y as f32;
            
            match &event.kind {
                PointerEventKind::Enter { .. } => {
                    if let Some(ref themed_pointer) = self.pointer {
                        let _ = themed_pointer.set_cursor(_conn, CursorIcon::Default);
                    }
                }
                PointerEventKind::Leave { .. } => {}
                PointerEventKind::Motion { .. } => {
                    let mut rebuild = false;
                    self.inner.handle_pointer_move(LogicalPosition::new(lx, ly), &mut rebuild);
                    if rebuild {
                        self.redraw = true;
                    }
                }
                PointerEventKind::Press { button, .. } => {
                    let btn = match *button {
                        272 => MouseButton::Left,
                        273 => MouseButton::Right,
                        274 => MouseButton::Middle,
                        _ => continue,
                    };
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
                    let h_scroll = horizontal.absolute as f32;
                    let v_scroll = vertical.absolute as f32;
                    let delta = MouseScrollDelta::LineDelta(-h_scroll / 10.0, -v_scroll / 10.0);
                    let mut rebuild = false;
                    self.inner.handle_mouse_wheel(&delta, LogicalPosition::new(lx, ly), &mut rebuild);
                    if rebuild {
                        self.redraw = true;
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

impl<A: Application> wayland_client::Dispatch<crate::protocol::zclear_inspector_v1::ZclearInspectorV1, ()> for EngineState<A> {
    fn event(
        _state: &mut Self,
        _proxy: &crate::protocol::zclear_inspector_v1::ZclearInspectorV1,
        _event: crate::protocol::zclear_inspector_v1::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {}
}

// Delegate macros for generic EngineState
delegate_compositor!(@<A: Application> EngineState<A>);
delegate_xdg_shell!(@<A: Application> EngineState<A>);
delegate_xdg_window!(@<A: Application> EngineState<A>);
delegate_shm!(@<A: Application> EngineState<A>);
delegate_seat!(@<A: Application> EngineState<A>);
delegate_pointer!(@<A: Application> EngineState<A>);
delegate_keyboard!(@<A: Application> EngineState<A>);
delegate_registry!(@<A: Application> EngineState<A>);
delegate_output!(@<A: Application> EngineState<A>);

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

    let font_system = FontSystem::new();
    let swash_cache = SwashCache::new();

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
        wgpu_surface: None,
        device: None,
        queue: None,
        config: None,
        render_pipeline: None,
        vertex_buffer: None,
        vertex_count: 0,
        overlay_vertex_buffer: None,
        overlay_vertex_count: 0,
        font_system,
        swash_cache,
        text_atlas: None,
        text_renderer: None,
        text_viewport: None,
        scale_factor: 1.0,
        logical_width: settings.width as f32,
        logical_height: settings.height as f32,
        exit: false,
        redraw: false,
        first_configure_received: false,
        ctrl_pressed: false,
        shift_pressed: false,
        pressed_key: None,
        sender,
    };

    event_queue.roundtrip(&mut engine_state).unwrap();

    let scale = detect_scale_factor(&engine_state.output_state);
    engine_state.scale_factor = scale;

    let surface = engine_state.compositor_state.create_surface(&qh);
    surface.set_buffer_scale(scale as i32);
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
    WaylandSource::new(conn, event_queue).insert(loop_handle.clone()).unwrap();

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

    let mut last_tick = std::time::Instant::now();
    loop {
        event_loop.dispatch(std::time::Duration::from_millis(16), &mut engine_state).unwrap();
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

        if engine_state.redraw {
            engine_state.redraw = false;
            if engine_state.first_configure_received {
                engine_state.render();
            }
        }
    }
}
