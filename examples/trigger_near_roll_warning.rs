//! Deliberately fires the debug-build "near-roll carve lost grouping" warning
//! (`near_roll_fallback_reason` in `backend/window_runner.rs`), so the loud
//! path can be seen working: a window plate opens the grouping window, a later
//! Bevel plate overlaps where the carve will go, and a full-ring recess hugging
//! the left roll is then rejected by the occlusion rule — groupable geometry,
//! still-open enclosing plate, shaded region in the roll band, dynamic
//! rejection. Expect one line on stderr:
//!
//!   plate-carve: near-roll recess (2,110 260x60) lost grouping — a later
//!   plate overlaps the carve's shaded region; ...
//!
//! Debug builds only (`cargo run -p cce-ui --example trigger_near_roll_warning`
//! inside a Wayland session — a cce-shadow instance works); a release build
//! compiles the warning out and prints nothing. Exits by itself after a few
//! frames.

use cce_ui::engine::{Application, EngineState, LogicalPosition, LogicalSize, WindowSettings};
use cce_ui::scene::layout::Rect;
use cce_ui::scene::paint::{DisplayList, PaintCtx};
use cce_ui::widget::{ElementState, KeyEvent, MouseButton, MouseScrollDelta};
use wayland_client::QueueHandle;

struct TriggerApp {
    ticks: u32,
}

impl Application for TriggerApp {
    type Message = ();

    fn new(
        _qh: &QueueHandle<EngineState<Self>>,
        _sender: calloop::channel::Sender<Self::Message>,
    ) -> Self {
        Self { ticks: 0 }
    }

    fn settings(&self) -> WindowSettings {
        WindowSettings {
            title: "near-roll trigger".into(),
            app_id: "cce-trigger-near-roll".into(),
            width: 800,
            height: 600,
            fullscreen: false,
            min_size: None,
        }
    }

    fn update(&mut self, _msg: (), _needs_rebuild: &mut bool, _exit: &mut bool) {}

    fn tick(&mut self, _dt: f32, needs_rebuild: &mut bool) {
        // Keep rendering a few frames so the first real present definitely
        // carried the display list, then leave. (The warning itself prints
        // once regardless of how many frames run.)
        self.ticks += 1;
        *needs_rebuild = true;
        if self.ticks > 20 {
            std::process::exit(0);
        }
    }

    fn display_list(&mut self, size: LogicalSize, _scale: f64) -> Option<DisplayList> {
        let mut pc = PaintCtx::new();
        let (w, h) = (size.width as f32, size.height as f32);
        let roll = cce_ui::layout::bevel_width();

        // 1. The window plate — opens the carve-grouping window.
        pc.plate(
            Rect { x: 0.0, y: 0.0, width: w, height: h },
            (12.0, 12.0, 12.0, 12.0),
            [0.13, 0.13, 0.16, 1.0],
            roll,
        );
        // 2. A later plate overlapping where the carve goes: the occlusion
        //    rule must reject grouping (the carve's shading would land beneath
        //    these pixels in the window plate's earlier draw).
        pc.bevel(
            Rect { x: 40.0, y: 100.0, width: 200.0, height: 80.0 },
            (6.0, 6.0, 6.0, 6.0),
            [0.20, 0.20, 0.25, 1.0],
            3.0,
        );
        // 3. Full-ring untinted recess hugging the left edge: its shaded
        //    region (rect inflated by depth/2 + 2) reaches into the window
        //    plate's roll band, so the fallback is the loud case.
        pc.recess(
            Rect { x: 2.0, y: 110.0, width: 260.0, height: 60.0 },
            (6.0, 6.0, 6.0, 6.0),
            6.0,
        );
        Some(pc.finish())
    }

    fn display_list_text(&self) -> bool {
        true
    }

    fn handle_pointer_move(&mut self, _pos: LogicalPosition, _needs_rebuild: &mut bool) {}
    fn handle_mouse_input(
        &mut self,
        _button: MouseButton,
        _state: ElementState,
        _pos: LogicalPosition,
        _needs_rebuild: &mut bool,
    ) -> Option<()> {
        None
    }
    fn handle_mouse_wheel(
        &mut self,
        _delta: &MouseScrollDelta,
        _pos: LogicalPosition,
        _needs_rebuild: &mut bool,
    ) {
    }
    fn handle_key_input(&mut self, _event: &KeyEvent, _needs_rebuild: &mut bool) -> Option<()> {
        None
    }
}

fn main() {
    cce_ui::engine::run::<TriggerApp>();
}
