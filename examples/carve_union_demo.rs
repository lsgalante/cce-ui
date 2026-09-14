//! A visual check of [`cce_ui::scene::paint::Prim::CarveUnion`] against the
//! per-box overlays it replaces. Left column: an L and a plus drawn as ONE
//! union carve (recess above, boss below). Right column: the same L and plus
//! drawn as one recess/boss per box — the crossing walls and stacked shading
//! the union exists to remove. Run inside a Wayland session (a cce-shadow
//! instance works): `cargo run --release -p cce-ui --example carve_union_demo`.

use cce_ui::engine::{Application, EngineState, LogicalPosition, LogicalSize, WindowSettings};
use cce_ui::scene::layout::Rect;
use cce_ui::scene::paint::{DisplayList, PaintCtx};
use cce_ui::widget::{ElementState, KeyEvent, MouseButton, MouseScrollDelta};
use wayland_client::QueueHandle;

struct DemoApp;

fn l_shape(x: f32, y: f32) -> Vec<(Rect, (f32, f32, f32, f32))> {
    let r = (8.0, 8.0, 8.0, 8.0);
    vec![
        (Rect { x, y, width: 260.0, height: 70.0 }, r),
        (Rect { x, y, width: 70.0, height: 200.0 }, r),
    ]
}

fn plus_shape(x: f32, y: f32) -> Vec<(Rect, (f32, f32, f32, f32))> {
    let r = (10.0, 10.0, 10.0, 10.0);
    vec![
        (Rect { x, y: y + 70.0, width: 220.0, height: 60.0 }, r),
        (Rect { x: x + 80.0, y, width: 60.0, height: 200.0 }, r),
    ]
}

impl Application for DemoApp {
    type Message = ();

    fn new(_qh: &QueueHandle<EngineState<Self>>, _sender: calloop::channel::Sender<()>) -> Self {
        Self
    }

    fn settings(&self) -> WindowSettings {
        WindowSettings {
            title: "carve union demo".into(),
            app_id: "cce-carve-union-demo".into(),
            // CCE_DEMO_W / CCE_DEMO_H override the size — handy when the
            // window doubles as a snap/tiling probe in a shadow session.
            width: std::env::var("CCE_DEMO_W").ok().and_then(|v| v.parse().ok()).unwrap_or(760),
            height: std::env::var("CCE_DEMO_H").ok().and_then(|v| v.parse().ok()).unwrap_or(560),
            fullscreen: false,
            min_size: None,
        }
    }

    fn update(&mut self, _msg: (), _needs_rebuild: &mut bool, _exit: &mut bool) {}

    fn tick(&mut self, _dt: f32, _needs_rebuild: &mut bool) {}

    fn display_list(&mut self, size: LogicalSize, _scale: f64) -> Option<DisplayList> {
        let mut pc = PaintCtx::new();
        let (w, h) = (size.width as f32, size.height as f32);
        pc.plate(
            Rect { x: 0.0, y: 0.0, width: w, height: h },
            (12.0, 12.0, 12.0, 12.0),
            [0.42, 0.44, 0.50, 1.0],
            cce_ui::layout::bevel_width(),
        );
        let wall = 10.0;
        // Left column: unions.
        pc.carve_union(l_shape(40.0, 40.0), wall, false);
        pc.carve_union(plus_shape(60.0, 300.0), wall, true);
        // Right column: one overlay per box — the look being replaced.
        for (r, radii) in l_shape(420.0, 40.0) {
            pc.recess(r, radii, wall);
        }
        for (r, radii) in plus_shape(440.0, 300.0) {
            pc.boss(r, radii, wall);
        }
        Some(pc.finish())
    }

    fn display_list_text(&self) -> bool {
        true
    }

    fn handle_pointer_move(&mut self, _pos: LogicalPosition, _needs_rebuild: &mut bool) {}
    fn handle_mouse_input(&mut self, _b: MouseButton, _s: ElementState, _p: LogicalPosition, _r: &mut bool) -> Option<()> {
        None
    }
    fn handle_mouse_wheel(&mut self, _d: &MouseScrollDelta, _p: LogicalPosition, _r: &mut bool) {}
    fn handle_key_input(&mut self, _e: &KeyEvent, _r: &mut bool) -> Option<()> {
        None
    }
}

fn main() {
    cce_ui::engine::run::<DemoApp>();
}
