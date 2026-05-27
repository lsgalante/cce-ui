use clear_ui::widget::{
    Button, Checkbox, ContentBg, Header, Panel, ProgressBar, Sidebar, Slider, Spinbox, StatusBar,
    TextLabel, Toggle, Widget,
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
    registry_handlers,
    output::{OutputHandler, OutputState},
    seat::{
        keyboard::KeyboardHandler,
        pointer::PointerHandler,
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
    protocol::{wl_keyboard, wl_output, wl_pointer, wl_seat, wl_shm, wl_surface},
    Connection, QueueHandle, Proxy,
};
use calloop::EventLoop;
use calloop_wayland_source::WaylandSource;

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
    async fn new(wayland_handle: &'static clear_ui::wayland::WaylandSurfaceHandle, pw: u32, ph: u32, scale: f64) -> Self {
        let lw = pw as f32 / scale as f32;
        let lh = ph as f32 / scale as f32;
        let sw = lw;
        let sh = lh;

        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::VULKAN,
            ..Default::default()
        });

        let surface = instance
            .create_surface(wayland_handle)
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

        let mut config = surface
            .get_default_config(&adapter, pw, ph)
            .expect("Failed to get surface config");
        config.present_mode = wgpu::PresentMode::Fifo;
        surface.configure(&device, &config);

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Shader"),
            source: wgpu::ShaderSource::Wgsl(clear_ui::SHADER.into()),
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
        if clear_ui::widget::context_menu::is_visible() {
            for (qx, qy, qw, qh, qc) in clear_ui::widget::context_menu::extra_quads() {
                verts.extend(quad_vertices(qx, qy, qw, qh, sw, sh, qc));
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

        if clear_ui::widget::context_menu::is_visible() {
            for label in clear_ui::widget::context_menu::text_labels() {
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

    fn resize(&mut self, width: u32, height: u32) {
        if width > 0 && height > 0 {
            self.physical_width = width;
            self.physical_height = height;
            self.width = width as f32 / self.scale as f32;
            self.height = height as f32 / self.scale as f32;
            self.config.width = width;
            self.config.height = height;
            self.surface.configure(&self.device, &self.config);
            self.positions = demo_positions(self.width, self.height);
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
                            r: 0.05,
                            g: 0.05,
                            b: 0.08,
                            a: 0.92,
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
        (0.0, sh - 28.0, sw, 28.0),         // 14 status_bar
    ]
}

struct PressedKey {
    logical_key: clear_ui::widget::Key,
    text: Option<String>,
    first_pressed: std::time::Instant,
    last_repeated: std::time::Instant,
}

fn is_repeatable_key(key: &clear_ui::widget::Key) -> bool {
    use clear_ui::widget::{Key, NamedKey};
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
    pointer: Option<wl_pointer::WlPointer>,
    keyboard: Option<wl_keyboard::WlKeyboard>,

    window: Option<XdgWindow>,
    surface: Option<wl_surface::WlSurface>,

    state: Option<State>,
    exit: bool,
    redraw: bool,
    ctrl_pressed: bool,
    shift_pressed: bool,
    pressed_key: Option<PressedKey>,
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
            let pointer = self.seat_state.get_pointer(qh, &seat).unwrap();
            self.pointer = Some(pointer);
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
        for event in events {
            let (x, y) = event.position;
            if let Some(state) = &mut self.state {
                state.cursor_x = x as f32;
                state.cursor_y = y as f32;
            }

            match &event.kind {
                PointerEventKind::Enter { .. } => {}
                PointerEventKind::Leave { .. } => {}
                PointerEventKind::Motion { .. } => {
                    if let Some(state) = &mut self.state {
                        let mut changed = false;
                        if clear_ui::widget::context_menu::is_visible() {
                            if clear_ui::widget::context_menu::cursor_moved(state.cursor_x, state.cursor_y) {
                                changed = true;
                            }
                        } else {
                            if let Some(idx) = state.drag_widget {
                                if state.widgets[idx].drag_update(state.cursor_x, state.cursor_y) {
                                    changed = true;
                                }
                            }
                            if state.drag_widget.is_none() {
                                for w in &mut state.widgets {
                                    if w.cursor_moved(state.cursor_x, state.cursor_y) {
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
                        272 => clear_ui::widget::MouseButton::Left,
                        273 => clear_ui::widget::MouseButton::Right,
                        274 => clear_ui::widget::MouseButton::Middle,
                        _ => continue,
                    };
                    if let Some(st) = &mut self.state {
                        let mut changed = false;
                        if clear_ui::widget::context_menu::is_visible() {
                            if clear_ui::widget::context_menu::mouse_input(btn, clear_ui::widget::ElementState::Pressed, st.cursor_x, st.cursor_y) {
                                changed = true;
                            }
                        } else {
                            if btn == clear_ui::widget::MouseButton::Left {
                                if let Some(old) = st.focused_widget.take() {
                                    st.widgets[old].unfocus();
                                }
                            }
                            for i in (0..st.widgets.len()).rev() {
                                if st.widgets[i].hit_test(st.cursor_x, st.cursor_y) {
                                    if st.widgets[i].mouse_input(
                                        btn,
                                        clear_ui::widget::ElementState::Pressed,
                                        st.cursor_x,
                                        st.cursor_y,
                                    ) {
                                        changed = true;
                                    }
                                    if btn == clear_ui::widget::MouseButton::Left && st.widgets[i].draggable() {
                                        st.widgets[i].drag_begin(st.cursor_x, st.cursor_y);
                                        st.drag_widget = Some(i);
                                    }
                                    if btn == clear_ui::widget::MouseButton::Left {
                                        st.widgets[i].focus();
                                        st.focused_widget = Some(i);
                                    }
                                    break;
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
                        272 => clear_ui::widget::MouseButton::Left,
                        273 => clear_ui::widget::MouseButton::Right,
                        274 => clear_ui::widget::MouseButton::Middle,
                        _ => continue,
                    };
                    if let Some(st) = &mut self.state {
                        let mut changed = false;
                        if clear_ui::widget::context_menu::is_visible() {
                            if clear_ui::widget::context_menu::mouse_input(btn, clear_ui::widget::ElementState::Released, st.cursor_x, st.cursor_y) {
                                changed = true;
                            }
                        } else {
                            if btn == clear_ui::widget::MouseButton::Left {
                                if let Some(idx) = st.drag_widget {
                                    st.widgets[idx].drag_end();
                                    st.drag_widget = None;
                                    changed = true;
                                }
                            }
                            for w in &mut st.widgets {
                                if w.mouse_input(btn, clear_ui::widget::ElementState::Released, st.cursor_x, st.cursor_y) {
                                    changed = true;
                                }
                            }
                            if btn == clear_ui::widget::MouseButton::Left {
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
                    }
                }
                PointerEventKind::Axis { horizontal, vertical, .. } => {
                    if let Some(state) = &mut self.state {
                        let h_scroll = horizontal.absolute as f32;
                        let v_scroll = vertical.absolute as f32;
                        
                        let delta = clear_ui::widget::MouseScrollDelta::LineDelta(-h_scroll / 10.0, -v_scroll / 10.0);
                        let mut changed = false;
                        for w in &mut state.widgets {
                            if w.mouse_wheel(&delta, state.cursor_x, state.cursor_y) {
                                changed = true;
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
    }

    fn press_key(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _keyboard: &wl_keyboard::WlKeyboard,
        _serial: u32,
        event: smithay_client_toolkit::seat::keyboard::KeyEvent,
    ) {
        self.handle_key(event, clear_ui::widget::ElementState::Pressed);
    }

    fn release_key(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _keyboard: &wl_keyboard::WlKeyboard,
        _serial: u32,
        event: smithay_client_toolkit::seat::keyboard::KeyEvent,
    ) {
        self.handle_key(event, clear_ui::widget::ElementState::Released);
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

impl AppState {
    fn handle_key(&mut self, event: smithay_client_toolkit::seat::keyboard::KeyEvent, state: clear_ui::widget::ElementState) {
        use clear_ui::widget::{Key, KeyEvent, NamedKey};
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

        if state == clear_ui::widget::ElementState::Pressed {
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
        } else if state == clear_ui::widget::ElementState::Released {
            if let Some(ref pk) = self.pressed_key {
                if pk.logical_key == custom_event.logical_key {
                    self.pressed_key = None;
                }
            }
        }

        if let Some(st) = &mut self.state {
            if let Some(idx) = st.focused_widget {
                let val = st.widgets[idx].value();
                let mut changed = st.widgets[idx].keyboard_input(&custom_event);
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
    let conn = Connection::connect_to_env().unwrap();
    let (globals, mut event_queue) = registry_queue_init(&conn).unwrap();
    let qh = event_queue.handle();

    let compositor_state = CompositorState::bind(&globals, &qh).unwrap();
    let xdg_shell_state = XdgShell::bind(&globals, &qh).unwrap();
    let shm_state = Shm::bind(&globals, &qh).unwrap();
    let seat_state = SeatState::new(&globals, &qh);
    let output_state = OutputState::new(&globals, &qh);

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
    };

    // Perform a roundtrip to populate output_state with active output scales
    event_queue.roundtrip(&mut app).unwrap();

    let scale = clear_ui::wayland::detect_scale_factor(&app.output_state);

    let surface = app.compositor_state.create_surface(&qh);
    surface.set_buffer_scale(scale as i32);

    let pw = (1024.0 * scale) as u32;
    let ph = (768.0 * scale) as u32;

    let window = app.xdg_shell_state.create_window(surface.clone(), WindowDecorations::None, &qh);
    window.set_title("Clear UI - Test Window");
    window.set_app_id("clear-ui");
    window.set_min_size(Some((pw, ph)));
    window.commit();

    let wayland_handle = Box::leak(Box::new(clear_ui::wayland::WaylandSurfaceHandle {
        display_ptr: conn.backend().display_id().as_ptr() as *mut std::ffi::c_void,
        surface_ptr: surface.id().as_ptr() as *mut std::ffi::c_void,
    }));

    let state = pollster::block_on(State::new(wayland_handle, pw, ph, scale));

    app.window = Some(window);
    app.surface = Some(surface);
    app.state = Some(state);

    let mut event_loop = EventLoop::try_new().unwrap();
    let loop_handle = event_loop.handle();
    WaylandSource::new(conn, event_queue).insert(loop_handle).unwrap();

    const KEY_REPEAT_DELAY: std::time::Duration = std::time::Duration::from_millis(500);
    const KEY_REPEAT_INTERVAL: std::time::Duration = std::time::Duration::from_millis(50);

    loop {
        event_loop
            .dispatch(std::time::Duration::from_millis(16), &mut app)
            .unwrap();
        if app.exit {
            break;
        }

        if let Some(ref mut pk) = app.pressed_key {
            let now = std::time::Instant::now();
            if now.duration_since(pk.first_pressed) >= KEY_REPEAT_DELAY {
                if now.duration_since(pk.last_repeated) >= KEY_REPEAT_INTERVAL {
                    pk.last_repeated = now;
                    let custom_event = clear_ui::widget::KeyEvent {
                        state: clear_ui::widget::ElementState::Pressed,
                        logical_key: pk.logical_key.clone(),
                        text: pk.text.clone(),
                        repeat: true,
                        ctrl: app.ctrl_pressed,
                        shift: app.shift_pressed,
                    };
                    if let Some(st) = &mut app.state {
                        if let Some(idx) = st.focused_widget {
                            let val = st.widgets[idx].value();
                            let mut changed = st.widgets[idx].keyboard_input(&custom_event);
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

        if app.redraw {
            app.redraw = false;
            if let Some(state) = &mut app.state {
                state.render();
            }
        }
    }
}
