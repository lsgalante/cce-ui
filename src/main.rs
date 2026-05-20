use std::sync::Arc;

use winit::application::ApplicationHandler;
use winit::event::{ElementState, MouseButton, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::{Window, WindowAttributes};
use taffy::prelude::TaffyMaxContent;

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

mod colors {
    pub const HEADER_BG: [f32; 4] = [0.08, 0.08, 0.12, 1.0];
    pub const HEADER_ACCENT: [f32; 4] = [0.60, 0.40, 0.20, 1.0];
    pub const SIDEBAR_BG: [f32; 4] = [0.10, 0.10, 0.13, 1.0];
    pub const CONTENT_BG: [f32; 4] = [0.13, 0.13, 0.16, 1.0];
    pub const PANEL_IDLE: [f32; 4] = [0.14, 0.70, 0.38, 1.0];
    pub const PANEL_DRAG: [f32; 4] = [0.24, 0.85, 0.50, 1.0];
    pub const BUTTON_IDLE: [f32; 4] = [0.20, 0.40, 0.65, 1.0];
    pub const BUTTON_HOVER: [f32; 4] = [0.30, 0.52, 0.78, 1.0];
    pub const BUTTON_PRESS: [f32; 4] = [0.12, 0.28, 0.50, 1.0];
    pub const STATUS_BG: [f32; 4] = [0.06, 0.06, 0.10, 1.0];
    pub const STATUS_ACCENT: [f32; 4] = [0.20, 0.20, 0.25, 1.0];
    pub const RESET_BTN_IDLE: [f32; 4] = [0.55, 0.20, 0.20, 1.0];
    pub const RESET_BTN_HOVER: [f32; 4] = [0.70, 0.30, 0.30, 1.0];
    pub const RESET_BTN_PRESS: [f32; 4] = [0.40, 0.12, 0.12, 1.0];
}

trait Widget {
    fn rect(&self) -> (f32, f32, f32, f32);
    fn set_rect(&mut self, x: f32, y: f32, w: f32, h: f32);

    fn hit_test(&self, px: f32, py: f32) -> bool {
        let (x, y, w, h) = self.rect();
        px >= x && px <= x + w && py >= y && py <= y + h
    }

    fn cursor_moved(&mut self, _px: f32, _py: f32) -> bool { false }
    fn mouse_input(&mut self, _button: MouseButton, _state: ElementState, _px: f32, _py: f32) -> bool { false }

    fn vertices(&self, sw: f32, sh: f32) -> Vec<Vertex> {
        let (x, y, w, h) = self.rect();
        quad_vertices(x, y, w, h, sw, sh, self.color()).to_vec()
    }

    fn color(&self) -> [f32; 4];

    fn is_dragging(&self) -> bool { false }
    fn drag_update(&mut self, _px: f32, _py: f32) -> bool { false }
    fn drag_begin(&mut self, _px: f32, _py: f32) {}
    fn drag_end(&mut self) {}
    fn take_click(&mut self) -> bool { false }
    fn draggable(&self) -> bool { false }
}

struct Header {
    x: f32, y: f32, w: f32, h: f32,
}

impl Header {
    fn new() -> Self { Self { x: 0.0, y: 0.0, w: 0.0, h: 0.0 } }
}

impl Widget for Header {
    fn rect(&self) -> (f32, f32, f32, f32) { (self.x, self.y, self.w, self.h) }
    fn set_rect(&mut self, x: f32, y: f32, w: f32, h: f32) { self.x = x; self.y = y; self.w = w; self.h = h; }
    fn color(&self) -> [f32; 4] { colors::HEADER_BG }

    fn vertices(&self, sw: f32, sh: f32) -> Vec<Vertex> {
        let (x, y, w, h) = self.rect();
        let mut verts = Vec::new();
        verts.extend(quad_vertices(x, y, w, h, sw, sh, colors::HEADER_BG));
        verts.extend(quad_vertices(x, y + h - 2.0, w, 2.0, sw, sh, colors::HEADER_ACCENT));
        verts
    }
}

struct StatusBar {
    x: f32, y: f32, w: f32, h: f32,
}

impl StatusBar {
    fn new() -> Self { Self { x: 0.0, y: 0.0, w: 0.0, h: 0.0 } }
}

impl Widget for StatusBar {
    fn rect(&self) -> (f32, f32, f32, f32) { (self.x, self.y, self.w, self.h) }
    fn set_rect(&mut self, x: f32, y: f32, w: f32, h: f32) { self.x = x; self.y = y; self.w = w; self.h = h; }
    fn color(&self) -> [f32; 4] { colors::STATUS_BG }

    fn vertices(&self, sw: f32, sh: f32) -> Vec<Vertex> {
        let (x, y, w, h) = self.rect();
        let mut verts = Vec::new();
        verts.extend(quad_vertices(x, y, w, h, sw, sh, colors::STATUS_BG));
        verts.extend(quad_vertices(x, y, w, 2.0, sw, sh, colors::STATUS_ACCENT));
        verts
    }
}

struct Panel {
    x: f32, y: f32, w: f32, h: f32,
    dragging: bool,
    drag_ox: f32, drag_oy: f32,
    drag_start_x: f32, drag_start_y: f32,
}

impl Panel {
    fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self { x, y, w, h, dragging: false, drag_ox: 0.0, drag_oy: 0.0, drag_start_x: 0.0, drag_start_y: 0.0 }
    }
}

impl Widget for Panel {
    fn rect(&self) -> (f32, f32, f32, f32) { (self.x, self.y, self.w, self.h) }
    fn set_rect(&mut self, x: f32, y: f32, w: f32, h: f32) { self.x = x; self.y = y; self.w = w; self.h = h; }
    fn color(&self) -> [f32; 4] { if self.dragging { colors::PANEL_DRAG } else { colors::PANEL_IDLE } }

    fn mouse_input(&mut self, button: MouseButton, state: ElementState, px: f32, py: f32) -> bool {
        if button != MouseButton::Left { return false; }
        match state {
            ElementState::Pressed => {
                if self.hit_test(px, py) {
                    self.drag_begin(px, py);
                    return true;
                }
            }
            ElementState::Released => {
                if self.dragging { self.drag_end(); return true; }
            }
        }
        false
    }

    fn is_dragging(&self) -> bool { self.dragging }
    fn draggable(&self) -> bool { true }

    fn drag_update(&mut self, px: f32, py: f32) -> bool {
        let nx = px - self.drag_ox;
        let ny = py - self.drag_oy;
        if (nx - self.x).abs() > 0.01 || (ny - self.y).abs() > 0.01 {
            self.x = nx;
            self.y = ny;
            return true;
        }
        false
    }

    fn drag_begin(&mut self, px: f32, py: f32) {
        self.dragging = true;
        self.drag_ox = px - self.x;
        self.drag_oy = py - self.y;
        self.drag_start_x = self.x;
        self.drag_start_y = self.y;
    }

    fn drag_end(&mut self) { self.dragging = false; }
}

enum ButtonKind {
    Primary,
    Reset,
}

struct Button {
    x: f32, y: f32, w: f32, h: f32,
    hovering: bool,
    pressed: bool,
    just_clicked: bool,
    kind: ButtonKind,
}

impl Button {
    fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self { x, y, w, h, hovering: false, pressed: false, just_clicked: false, kind: ButtonKind::Primary }
    }

    fn new_reset(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self { x, y, w, h, hovering: false, pressed: false, just_clicked: false, kind: ButtonKind::Reset }
    }
}

impl Widget for Button {
    fn rect(&self) -> (f32, f32, f32, f32) { (self.x, self.y, self.w, self.h) }
    fn set_rect(&mut self, x: f32, y: f32, w: f32, h: f32) { self.x = x; self.y = y; self.w = w; self.h = h; }
    fn color(&self) -> [f32; 4] {
        match self.kind {
            ButtonKind::Primary => {
                if self.pressed { colors::BUTTON_PRESS }
                else if self.hovering { colors::BUTTON_HOVER }
                else { colors::BUTTON_IDLE }
            }
            ButtonKind::Reset => {
                if self.pressed { colors::RESET_BTN_PRESS }
                else if self.hovering { colors::RESET_BTN_HOVER }
                else { colors::RESET_BTN_IDLE }
            }
        }
    }

    fn cursor_moved(&mut self, px: f32, py: f32) -> bool {
        let was = self.hovering;
        self.hovering = self.hit_test(px, py);
        was != self.hovering
    }

    fn mouse_input(&mut self, button: MouseButton, state: ElementState, px: f32, py: f32) -> bool {
        if button != MouseButton::Left { return false; }
        match state {
            ElementState::Pressed => {
                if self.hit_test(px, py) {
                    self.pressed = true;
                    return true;
                }
            }
            ElementState::Released => {
                if self.pressed && self.hit_test(px, py) {
                    self.just_clicked = true;
                }
                let was = self.pressed;
                self.pressed = false;
                return was;
            }
        }
        false
    }

    fn take_click(&mut self) -> bool {
        if self.just_clicked { self.just_clicked = false; true } else { false }
    }
}

struct Sidebar {
    x: f32, y: f32, w: f32, h: f32,
}

impl Sidebar {
    fn new(w: f32) -> Self { Self { x: 0.0, y: 0.0, w, h: 0.0 } }
}

impl Widget for Sidebar {
    fn rect(&self) -> (f32, f32, f32, f32) { (self.x, self.y, self.w, self.h) }
    fn set_rect(&mut self, x: f32, y: f32, w: f32, h: f32) { self.x = x; self.y = y; self.w = w; self.h = h; }
    fn color(&self) -> [f32; 4] { colors::SIDEBAR_BG }
}

struct ContentBg {
    x: f32, y: f32, w: f32, h: f32,
}

impl ContentBg {
    fn new() -> Self { Self { x: 0.0, y: 0.0, w: 0.0, h: 0.0 } }
}

impl Widget for ContentBg {
    fn rect(&self) -> (f32, f32, f32, f32) { (self.x, self.y, self.w, self.h) }
    fn set_rect(&mut self, x: f32, y: f32, w: f32, h: f32) { self.x = x; self.y = y; self.w = w; self.h = h; }
    fn color(&self) -> [f32; 4] { colors::CONTENT_BG }
}

use taffy::geometry::{Rect, Size};
use taffy::style::{Dimension, FlexDirection, LengthPercentage, Style};

struct Layout {
    tree: taffy::TaffyTree<()>,
    root: taffy::NodeId,
    widget_nodes: Vec<taffy::NodeId>,
}

impl Layout {
    fn new(width: f32, height: f32) -> Self {
        let mut tree = taffy::TaffyTree::new();

        let header = tree.new_leaf(Style {
            size: Size { width: Dimension::percent(1.0), height: Dimension::length(40.0) },
            ..Default::default()
        }).unwrap();

        let sidebar = tree.new_leaf(Style {
            size: Size { width: Dimension::length(60.0), height: Dimension::percent(1.0) },
            ..Default::default()
        }).unwrap();

        let content_bg = tree.new_leaf(Style {
            flex_grow: 1.0,
            size: Size { width: Dimension::percent(1.0), height: Dimension::percent(1.0) },
            ..Default::default()
        }).unwrap();

        let btn_a = tree.new_leaf(Style {
            size: Size { width: Dimension::length(140.0), height: Dimension::length(40.0) },
            ..Default::default()
        }).unwrap();

        let btn_b = tree.new_leaf(Style {
            size: Size { width: Dimension::length(140.0), height: Dimension::length(40.0) },
            ..Default::default()
        }).unwrap();

        let btn_c = tree.new_leaf(Style {
            size: Size { width: Dimension::length(140.0), height: Dimension::length(40.0) },
            ..Default::default()
        }).unwrap();

        let panel = tree.new_leaf(Style {
            size: Size { width: Dimension::length(400.0), height: Dimension::length(250.0) },
            ..Default::default()
        }).unwrap();

        let click_me = tree.new_leaf(Style {
            size: Size { width: Dimension::length(140.0), height: Dimension::length(40.0) },
            ..Default::default()
        }).unwrap();

        let reset = tree.new_leaf(Style {
            size: Size { width: Dimension::length(140.0), height: Dimension::length(40.0) },
            ..Default::default()
        }).unwrap();

        let status_bar = tree.new_leaf(Style {
            size: Size { width: Dimension::percent(1.0), height: Dimension::length(28.0) },
            ..Default::default()
        }).unwrap();

        let btn_row_1 = tree.new_with_children(
            Style {
                display: taffy::style::Display::Flex,
                flex_direction: FlexDirection::Row,
                gap: Size { width: LengthPercentage::length(10.0), height: LengthPercentage::length(0.0) },
                ..Default::default()
            },
            &[btn_a, btn_b, btn_c],
        ).unwrap();

        let btn_row_2 = tree.new_with_children(
            Style {
                display: taffy::style::Display::Flex,
                flex_direction: FlexDirection::Row,
                gap: Size { width: LengthPercentage::length(10.0), height: LengthPercentage::length(0.0) },
                ..Default::default()
            },
            &[click_me, reset],
        ).unwrap();

        let _content = tree.new_with_children(
            Style {
                display: taffy::style::Display::Flex,
                flex_direction: FlexDirection::Column,
                flex_grow: 1.0,
                padding: Rect {
                    top: LengthPercentage::length(20.0),
                    left: LengthPercentage::length(20.0),
                    bottom: LengthPercentage::length(0.0),
                    right: LengthPercentage::length(0.0),
                },
                gap: Size { width: LengthPercentage::length(0.0), height: LengthPercentage::length(12.0) },
                ..Default::default()
            },
            &[btn_row_1, panel, btn_row_2],
        ).unwrap();

        let body = tree.new_with_children(
            Style {
                display: taffy::style::Display::Flex,
                flex_direction: FlexDirection::Row,
                flex_grow: 1.0,
                size: Size { width: Dimension::percent(1.0), height: Dimension::auto() },
                ..Default::default()
            },
            &[sidebar, content_bg],
        ).unwrap();

        let root = tree.new_with_children(
            Style {
                display: taffy::style::Display::Flex,
                flex_direction: FlexDirection::Column,
                size: Size { width: Dimension::length(width), height: Dimension::length(height) },
                ..Default::default()
            },
            &[header, body, status_bar],
        ).unwrap();

        let widget_nodes = vec![
            header,
            sidebar,
            content_bg,
            btn_a,
            btn_b,
            btn_c,
            panel,
            click_me,
            reset,
            status_bar,
        ];

        let mut layout = Self { tree, root, widget_nodes };
        layout.compute(width, height);
        layout
    }

    fn compute(&mut self, width: f32, height: f32) {
        self.tree.set_style(self.root, Style {
            size: Size { width: Dimension::length(width), height: Dimension::length(height) },
            ..Default::default()
        }).unwrap();

        self.tree.compute_layout(self.root, Size::MAX_CONTENT).unwrap();
    }

    fn widget_rect(&self, node_id: taffy::NodeId) -> (f32, f32, f32, f32) {
        let layout = self.tree.layout(node_id).unwrap();
        let x = layout.location.x;
        let y = layout.location.y;
        let w = layout.size.width;
        let h = layout.size.height;
        (x, y, w, h)
    }

    fn resize(&mut self, width: f32, height: f32) {
        self.compute(width, height);
    }
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
    layout: Layout,

    drag_widget: Option<usize>,
    click_count: u32,

    cursor_x: f32,
    cursor_y: f32,

    width: u32,
    height: u32,
}

impl State {
    async fn new(window: Arc<Window>) -> Self {
        let size = window.inner_size();
        let sw = size.width as f32;
        let sh = size.height as f32;

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
            .get_default_config(&adapter, size.width.max(1), size.height.max(1))
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

        let layout = Layout::new(sw, sh);

        let widgets: Vec<Box<dyn Widget>> = vec![
            Box::new(Header::new()),                        // 0
            Box::new(Sidebar::new(60.0)),                   // 1
            Box::new(ContentBg::new()),                     // 2
            Box::new(Button::new(0.0, 0.0, 140.0, 40.0)),  // 3
            Box::new(Button::new(0.0, 0.0, 140.0, 40.0)),  // 4
            Box::new(Button::new(0.0, 0.0, 140.0, 40.0)),  // 5
            Box::new(Panel::new(0.0, 0.0, 400.0, 250.0)),  // 6
            Box::new(Button::new(0.0, 0.0, 140.0, 40.0)),  // 7
            Box::new(Button::new_reset(0.0, 0.0, 140.0, 40.0)), // 8
            Box::new(StatusBar::new()),                     // 9
        ];

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
            layout,
            drag_widget: None,
            click_count: 0,
            cursor_x: 0.0,
            cursor_y: 0.0,
            width: size.width,
            height: size.height,
        };

        state.apply_layout();
        state.upload_vertices();
        state
    }

    fn apply_layout(&mut self) {
        for (i, node_id) in self.layout.widget_nodes.iter().enumerate() {
            if let Some(widget) = self.widgets.get_mut(i) {
                if widget.is_dragging() {
                    continue;
                }
                let (x, y, w, h) = self.layout.widget_rect(*node_id);
                widget.set_rect(x, y, w, h);
            }
        }
    }

    fn collect_vertices(&self) -> Vec<Vertex> {
        let sw = self.width as f32;
        let sh = self.height as f32;
        let mut verts = Vec::new();
        for w in &self.widgets {
            verts.extend(w.vertices(sw, sh));
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

    fn resize(&mut self, new_size: winit::dpi::PhysicalSize<u32>) {
        if new_size.width > 0 && new_size.height > 0 {
            self.width = new_size.width;
            self.height = new_size.height;
            self.config.width = new_size.width;
            self.config.height = new_size.height;
            self.surface.configure(&self.device, &self.config);
            self.layout.resize(new_size.width as f32, new_size.height as f32);
            self.apply_layout();
            self.upload_vertices();
        }
    }

    fn handle_event(&mut self, event: &WindowEvent) -> bool {
        match event {
            WindowEvent::CursorMoved { position, .. } => {
                self.cursor_x = position.x as f32;
                self.cursor_y = position.y as f32;
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
                        for i in (0..self.widgets.len()).rev() {
                            if self.widgets[i].hit_test(self.cursor_x, self.cursor_y) {
                                if self.widgets[i].mouse_input(*button, *btn_state, self.cursor_x, self.cursor_y) {
                                    changed = true;
                                }
                                if self.widgets[i].draggable() {
                                    self.widgets[i].drag_begin(self.cursor_x, self.cursor_y);
                                    self.drag_widget = Some(i);
                                }
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
                        for w in &mut self.widgets {
                            if w.take_click() {
                                self.click_count += 1;
                            }
                        }
                    }
                }
                changed
            }
            _ => false,
        }
    }

    fn render(&mut self) {
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

            pass.set_pipeline(&self.render_pipeline);
            pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
            pass.draw(0..self.vertex_count, 0..1);
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
                        .with_title("UI Framework - Test Window")
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
                            "UI Framework - Test Window  |  clicks: {}", clicks
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

fn main() {
    let event_loop = EventLoop::new().unwrap();
    event_loop.set_control_flow(ControlFlow::Poll);

    let mut app = App::new();
    event_loop.run_app(&mut app).unwrap();
}
