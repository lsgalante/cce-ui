use std::sync::Arc;

use winit::application::ApplicationHandler;
use winit::event::{ElementState, MouseButton, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::{Window, WindowAttributes};

use clear_ui::widget::{Button, Checkbox, ContentBg, Header, Panel, ProgressBar, Sidebar, Slider, Spinbox, StatusBar, TextLabel, Toggle, Widget};

use glyphon::{
    Attrs, Buffer, Cache, FontSystem, Metrics, Resolution, SwashCache, TextArea, TextAtlas,
    TextBounds, TextRenderer, Viewport,
};

#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Vertex {
    position: [f32; 2],
    color: [f32; 4],
}

impl Vertex {
    const ATTRIBS: [wgpu::VertexAttribute; 2] = wgpu::vertex_attr_array![
        0 => Float32x2,
        1 => Float32x4,
    ];

    fn desc() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Vertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &Self::ATTRIBS,
        }
    }
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
        Vertex { position: [x0, y0], color },
        Vertex { position: [x1, y0], color },
        Vertex { position: [x0, y1], color },
        Vertex { position: [x1, y0], color },
        Vertex { position: [x1, y1], color },
        Vertex { position: [x0, y1], color },
    ]
}

fn widget_vertices(w: &dyn Widget, sw: f32, sh: f32) -> Vec<Vertex> {
    let (x, y, ww, h) = w.rect();
    quad_vertices(x, y, ww, h, sw, sh, w.color()).to_vec()
}

fn make_text_buffer(font_system: &mut FontSystem, text: &str, size: f32) -> Buffer {
    let metrics = Metrics::new(size, size * 1.4);
    let mut buffer = Buffer::new(font_system, metrics);
    buffer.set_text(font_system, text, Attrs::new(), glyphon::Shaping::Advanced);
    buffer.shape_until_scroll(font_system, true);
    buffer
}

struct State {
    window: Arc<Window>,
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    render_pipeline: wgpu::RenderPipeline,
    vertex_buffer: wgpu::Buffer,
    vertex_count: u32,

    widgets: Vec<Box<dyn Widget>>,
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
}

impl State {
    async fn new(window: Arc<Window>) -> Self {
        let scale = window.scale_factor();
        let physical_size = window.inner_size();
        let pw = physical_size.width.max(1);
        let ph = physical_size.height.max(1);
        let lw = pw as f32 / scale as f32;
        let lh = ph as f32 / scale as f32;
        let sw = lw;
        let sh = lh;

        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::VULKAN,
            ..Default::default()
        });

        let surface = instance
            .create_surface(window.clone())
            .expect("Failed to create surface");

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await
            .expect("Failed to find adapter");

        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    label: Some("GPU Device"),
                    required_features: wgpu::Features::empty(),
                    required_limits: wgpu::Limits::downlevel_webgl2_defaults()
                        .using_resolution(adapter.limits()),
                    memory_hints: wgpu::MemoryHints::MemoryUsage,
                },
                None,
            )
            .await
            .expect("Failed to create device");

        let config = surface
            .get_default_config(&adapter, pw, ph)
            .expect("Failed to get surface config");
        surface.configure(&device, &config);

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shader.wgsl").into()),
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
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
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

        // Initialize text rendering
        let mut font_system = FontSystem::new();
        let swash_cache = SwashCache::new();
        let cache = Cache::new(&device);
        let mut text_atlas = TextAtlas::new(&device, &queue, &cache, config.format);
        let text_renderer = TextRenderer::new(&mut text_atlas, &device, wgpu::MultisampleState::default(), None);

        let mut text_viewport = Viewport::new(&device, &cache);
        text_viewport.update(&queue, Resolution { width: pw, height: ph });

        let label_buffer = make_text_buffer(&mut font_system, "Hello, Clear UI!", 16.0);
        let status_buffer = make_text_buffer(&mut font_system, "Click a button to interact", 12.0);

        let widgets: Vec<Box<dyn Widget>> = vec![
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
            Box::new(StatusBar::new()),
            Box::new(Spinbox::new(0, -10, 10, 1)),
        ];

        let positions = demo_positions(sw, sh);

        let vertex_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Vertex Buffer"),
            size: 1,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let mut state = Self {
            window,
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
        };

        state.apply_layout();
        state.upload_vertices();
        state
    }

    fn apply_layout(&mut self) {
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

    fn collect_vertices(&self) -> Vec<Vertex> {
        let sw = self.width;
        let sh = self.height;
        let mut verts = Vec::new();
        for w in &self.widgets {
            verts.extend(widget_vertices(w.as_ref(), sw, sh));
            for (qx, qy, qw, qh, qc) in w.extra_quads() {
                verts.extend(quad_vertices(qx, qy, qw, qh, sw, sh, qc));
            }
        }
        verts
    }

    fn upload_vertices(&mut self) {
        let verts = self.collect_vertices();
        self.vertex_count = verts.len() as u32;
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
            ..
        } = self;

        let viewport = Resolution { width: *physical_width, height: *physical_height };
        text_viewport.update(queue, viewport);

        let scale_f32 = *scale as f32;

        let mut areas: Vec<TextArea> = vec![
            TextArea {
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
            },
            TextArea {
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
            },
        ];

        let mut widget_buffers: Vec<Buffer> = Vec::new();
        let mut widget_labels: Vec<TextLabel> = Vec::new();
        for w in self.widgets.iter() {
            for label in w.text_labels() {
                widget_buffers.push(make_text_buffer(font_system, &label.text, label.font_size));
                widget_labels.push(label);
            }
        }

        for (buf, label) in widget_buffers.iter().zip(widget_labels.iter()) {
            areas.push(TextArea {
                buffer: buf,
                left: label.x * scale_f32,
                top: label.y * scale_f32,
                scale: scale_f32,
                bounds: TextBounds {
                    left: 0,
                    top: 0,
                    right: *physical_width as i32,
                    bottom: *physical_height as i32,
                },
                default_color: glyphon::Color::rgb(label.color[0], label.color[1], label.color[2]),
                custom_glyphs: &[],
            });
        }

        text_renderer
            .prepare(device, queue, font_system, text_atlas, text_viewport, areas, swash_cache)
            .unwrap();
    }

    fn resize(&mut self, new_size: winit::dpi::PhysicalSize<u32>) {
        if new_size.width > 0 && new_size.height > 0 {
            self.physical_width = new_size.width;
            self.physical_height = new_size.height;
            self.width = new_size.width as f32 / self.scale as f32;
            self.height = new_size.height as f32 / self.scale as f32;
            self.config.width = new_size.width;
            self.config.height = new_size.height;
            self.surface.configure(&self.device, &self.config);
            self.positions = demo_positions(self.width, self.height);
            self.apply_layout();
            self.upload_vertices();
        }
    }

    fn handle_event(&mut self, event: &WindowEvent) -> bool {
        match event {
            WindowEvent::MouseWheel { delta, .. } => {
                let mut changed = false;
                for w in &mut self.widgets {
                    if w.mouse_wheel(delta, self.cursor_x, self.cursor_y) {
                        changed = true;
                    }
                }
                changed
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.cursor_x = position.x as f32 / self.scale as f32;
                self.cursor_y = position.y as f32 / self.scale as f32;
                let mut changed = false;

                if let Some(idx) = self.drag_widget {
                    if self.widgets[idx].drag_update(self.cursor_x, self.cursor_y) {
                        changed = true;
                    }
                }

                if self.drag_widget.is_none() {
                    for w in &mut self.widgets {
                        if w.cursor_moved(self.cursor_x, self.cursor_y) {
                            changed = true;
                        }
                    }
                }
                changed
            }
            WindowEvent::MouseInput { state: btn_state, button, .. } => {
                if *button != MouseButton::Left { return false; }
                let mut changed = false;

                match btn_state {
                    ElementState::Pressed => {
                        if let Some(old) = self.focused_widget.take() {
                            self.widgets[old].unfocus();
                        }
                        for i in (0..self.widgets.len()).rev() {
                            if self.widgets[i].hit_test(self.cursor_x, self.cursor_y) {
                                if self.widgets[i].mouse_input(*button, *btn_state, self.cursor_x, self.cursor_y) {
                                    changed = true;
                                }
                                if self.widgets[i].draggable() {
                                    self.widgets[i].drag_begin(self.cursor_x, self.cursor_y);
                                    self.drag_widget = Some(i);
                                }
                                self.widgets[i].focus();
                                self.focused_widget = Some(i);
                                break;
                            }
                        }
                    }
                    ElementState::Released => {
                        if let Some(idx) = self.drag_widget {
                            self.widgets[idx].drag_end();
                            self.drag_widget = None;
                            changed = true;
                        }
                        for w in &mut self.widgets {
                            if w.mouse_input(*button, *btn_state, self.cursor_x, self.cursor_y) {
                                changed = true;
                            }
                        }
                        let mut clicked = false;
                        for w in &mut self.widgets {
                            if w.take_click() {
                                clicked = true;
                            }
                        }
                        if clicked {
                            self.click_count += 1;
                            self.update_status_text(&format!("Clicks: {}", self.click_count));
                        }
                    }
                }
                changed
            }
            WindowEvent::KeyboardInput { event, .. } => {
                if let Some(idx) = self.focused_widget {
                    let val = self.widgets[idx].value();
                    let changed = self.widgets[idx].keyboard_input(event);
                    if self.widgets[idx].value() != val {
                        changed || true
                    } else {
                        changed
                    }
                } else {
                    false
                }
            }
            _ => false,
        }
    }

    fn render(&mut self) {
        self.prepare_text();

        let output = match self.surface.get_current_texture() {
            Ok(t) => t,
            Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                self.surface.configure(&self.device, &self.config);
                return;
            }
            Err(wgpu::SurfaceError::Timeout) => return,
            Err(e) => {
                eprintln!("Surface error: {e:?}");
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
                            r: 0.06, g: 0.06, b: 0.08, a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            // Draw colored quads
            pass.set_pipeline(&self.render_pipeline);
            pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
            pass.draw(0..self.vertex_count, 0..1);

            // Draw text
            self.text_renderer.render(&self.text_atlas, &self.text_viewport, &mut pass).unwrap();
        }

        self.queue.submit(std::iter::once(encoder.finish()));
        self.window.pre_present_notify();
        output.present();
    }
}

struct App {
    state: Option<State>,
}

impl App {
    fn new() -> Self { Self { state: None } }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.state.is_some() { return; }

        let window = Arc::new(
            event_loop
                .create_window(
                    WindowAttributes::default()
                        .with_title("Clear UI - Test Window")
                        .with_inner_size(winit::dpi::LogicalSize::new(1024, 768)),
                )
                .unwrap(),
        );

        let state = pollster::block_on(State::new(window));
        self.state = Some(state);
        self.state.as_ref().unwrap().window.request_redraw();
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: winit::window::WindowId,
        event: WindowEvent,
    ) {
        let needs_redraw = match event {
            WindowEvent::CloseRequested => {
                event_loop.exit();
                true
            }
            WindowEvent::Resized(size) => {
                if let Some(state) = &mut self.state {
                    state.resize(size);
                }
                true
            }
            WindowEvent::RedrawRequested => {
                if let Some(state) = &mut self.state {
                    state.render();
                    state.window.request_redraw();
                }
                true
            }
            WindowEvent::ScaleFactorChanged { scale_factor, mut inner_size_writer } => {
                if let Some(state) = &mut self.state {
                    let new_physical = winit::dpi::PhysicalSize::new(
                        (state.width as f64 * scale_factor) as u32,
                        (state.height as f64 * scale_factor) as u32,
                    );
                    let _ = inner_size_writer.request_inner_size(new_physical);
                    state.scale = scale_factor;
                    state.resize(new_physical);
                    state.window.request_redraw();
                }
                true
            }
            _ => {
                if let Some(state) = &mut self.state {
                    let prev = state.click_count;
                    let changed = state.handle_event(&event);
                    if changed {
                        state.upload_vertices();
                    }
                    if state.click_count != prev {
                        let clicks = state.click_count;
                        state.window.set_title(&format!(
                            "Clear UI - Test Window  |  clicks: {}", clicks
                        ));
                    }
                    changed
                } else {
                    false
                }
            }
        };
        if needs_redraw {
            if let Some(state) = &mut self.state {
                state.window.request_redraw();
            }
        }
    }
}

fn demo_positions(sw: f32, sh: f32) -> Vec<(f32, f32, f32, f32)> {
    vec![
        (0.0, 0.0, sw, 40.0),                        // 0 header
        (0.0, 40.0, 60.0, sh - 68.0),                // 1 sidebar
        (60.0, 40.0, sw - 60.0, sh - 68.0),          // 2 content_bg
        (70.0, 50.0, 140.0, 40.0),                   // 3 btn_a
        (220.0, 50.0, 140.0, 40.0),                  // 4 btn_b
        (370.0, 50.0, 140.0, 40.0),                  // 5 btn_c
        (70.0, 100.0, 400.0, 250.0),                 // 6 panel
        (70.0, 360.0, 140.0, 40.0),                  // 7 click_me
        (220.0, 360.0, 140.0, 40.0),                 // 8 reset
        (70.0, 410.0, 24.0, 24.0),                   // 9 checkbox
        (104.0, 410.0, 48.0, 24.0),                  // 10 toggle
        (162.0, 410.0, 160.0, 24.0),                 // 11 progress_bar
        (70.0, 444.0, 300.0, 32.0),                  // 12 slider
        (70.0, 486.0, 120.0, 32.0),                  // 13 spinbox
        (0.0, sh - 28.0, sw, 28.0),                  // 14 status_bar
    ]
}

fn main() {
    let event_loop = EventLoop::new().unwrap();
    event_loop.set_control_flow(ControlFlow::Poll);

    let mut app = App::new();
    event_loop.run_app(&mut app).unwrap();
}
