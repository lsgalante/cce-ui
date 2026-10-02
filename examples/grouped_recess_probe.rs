//! A visual check that a recess GROUPED into its plate as a CSG feature
//! shades like the same recess drawn as an overlay. Left plate: the recesses
//! group (nothing else is painted between the plate and them). Right plate:
//! the same recesses, but a transparent quad painted first closes the plate's
//! grouping window, so each one falls back to the overlay. The two columns
//! should read alike on every device — compare `CCE_VK_DEVICE=discrete`
//! against the default. `CCE_PLATE_DEBUG=1` prints the grouping verdicts.
//! Run inside a Wayland session (a cce-shadow instance works):
//! `cargo run --release -p cce-ui --example grouped_recess_probe`.

use cce_ui::engine::{Application, EngineState, LogicalPosition, LogicalSize, WindowSettings};
use cce_ui::scene::layout::Rect;
use cce_ui::scene::paint::{DisplayList, PaintCtx};
use cce_ui::scene::Material;
use cce_ui::widget::{ElementState, KeyEvent, MouseButton, MouseScrollDelta};
use wayland_client::QueueHandle;

struct ProbeApp;

/// The probe's recesses, relative to a plate's top-left: (rect, radius,
/// depth) at the sizes the parameter pane uses — a text box, a toggle's
/// track, a colour well — and a couple of deeper ones.
fn recesses(x: f32, y: f32) -> Vec<(Rect, f32, f32)> {
    let w = cce_ui::layout::bevel_width();
    let mut out = Vec::new();
    let mut yy = y + 30.0;
    for (h, wide) in [(20.0, 260.0), (20.0, 120.0), (28.0, 260.0), (40.0, 260.0), (70.0, 260.0)] {
        let depth = w.min(h * 0.2);
        out.push((Rect { x: x + 30.0, y: yy, width: wide, height: h }, (h * 0.5).min(8.0), depth));
        yy += h + 24.0;
    }
    out
}

impl Application for ProbeApp {
    type Message = ();

    fn new(_qh: &QueueHandle<EngineState<Self>>, _sender: calloop::channel::Sender<()>) -> Self {
        Self
    }

    fn settings(&self) -> WindowSettings {
        WindowSettings {
            title: "grouped recess probe".into(),
            app_id: "cce-grouped-recess-probe".into(),
            width: 720,
            height: 400,
            fullscreen: false,
            min_size: None,
        }
    }

    fn update(&mut self, _msg: (), _needs_rebuild: &mut bool, _exit: &mut bool) {}

    fn tick(&mut self, _dt: f32, _needs_rebuild: &mut bool) {}

    fn display_list(&mut self, size: LogicalSize, _scale: f64) -> Option<DisplayList> {
        let mut pc = PaintCtx::new();
        let (w, h) = (size.width as f32, size.height as f32);
        let half = (w - 30.0) * 0.5;
        let bevel = cce_ui::layout::bevel_width();
        for (i, x) in [10.0, 20.0 + half].into_iter().enumerate() {
            let plate = Rect { x, y: 10.0, width: half, height: h - 20.0 };
            pc.plate(plate, (16.0, 16.0, 16.0, 16.0), &Material::pane(), bevel);
            if i == 1 {
                // Ordinary geometry between the plate and its carves closes
                // the grouping window: every recess below is an overlay.
                pc.quad(Rect { x: x + 1.0, y: 11.0, width: 1.0, height: 1.0 }, [0.0; 4]);
            }
            for (r, radius, depth) in recesses(x, 10.0) {
                pc.recess(r, (radius, radius, radius, radius), depth);
            }
            // A boss, and a recess running into the plate's roll — the
            // junction grouping exists for (the overlay fades out there
            // instead, so the two columns differ by design at its right end).
            let boss = Rect { x: x + 30.0, y: h - 60.0, width: 120.0, height: 30.0 };
            pc.boss(boss, (8.0, 8.0, 8.0, 8.0), 6.0);
            let edge = Rect { x: x + 180.0, y: h - 60.0, width: half - 180.0 - 2.0, height: 30.0 };
            pc.recess(edge, (8.0, 8.0, 8.0, 8.0), 6.0);
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
    cce_ui::engine::run::<ProbeApp>();
}
