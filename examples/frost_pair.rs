//! Three frosted plates, one recipe each, over a bright/dark checker — the
//! per-plate frost exit test of `docs/rfc-material.md` § 7 step 3.
//!
//! Left: compression 0 (the backdrop's luminance passes through). Middle:
//! compression 0.85 (the plate holds its own key). Right: a CLEAR plate
//! (radius 0 — one clean sample, tinted). Same tint on all three, the
//! designer's `#05050840`. Measured from a shadow screenshot as the swing in
//! mean luminance between the bright and dark checker columns under each
//! plate: the middle plate must swing far less than the left one, and the
//! right one must show the checker's edges sharp.
//!
//! Run it ONLY in a shadow session (`cce-shadow spawn`), never from the live
//! shell.

use cce_ui::engine::{Application, EngineState, LogicalPosition, LogicalSize, WindowSettings};
use cce_ui::scene::layout::Rect;
use cce_ui::scene::paint::{DisplayList, PaintCtx, PlateSpec};
use cce_ui::scene::{Frost, Material};
use cce_ui::widget::{ElementState, KeyEvent, MouseButton, MouseScrollDelta};
use wayland_client::QueueHandle;

/// Logical layout: 12 checker columns of `COL` px; three plates in thirds.
pub const COL: f32 = 35.0;
pub const W: f32 = 12.0 * COL;
pub const H: f32 = 210.0;
pub const PLATE_Y: f32 = 30.0;
pub const PLATE_H: f32 = H - 60.0;
pub const PLATE_INSET: f32 = 10.0;

/// The three recipes, left to right: (compression, radius).
pub const RECIPES: [(f32, f32); 3] = [(0.0, Frost::DEFAULT_RADIUS), (0.85, Frost::DEFAULT_RADIUS), (0.0, 0.0)];

/// The designer's tint, `#05050840` gamma-decoded.
pub const TINT: [f32; 4] = [0.0015, 0.0015, 0.0024, 0.25];

pub fn plate_rect(i: usize) -> Rect {
    let third = W / 3.0;
    Rect { x: PLATE_INSET + i as f32 * third, y: PLATE_Y, width: third - 2.0 * PLATE_INSET, height: PLATE_H }
}

struct FrostPair;

impl Application for FrostPair {
    type Message = ();

    fn new(_qh: &QueueHandle<EngineState<Self>>, _sender: calloop::channel::Sender<Self::Message>) -> Self {
        FrostPair
    }

    fn settings(&self) -> WindowSettings {
        WindowSettings {
            title: "frost pair".to_string(),
            app_id: "cce-frost-pair".to_string(),
            width: W as u32,
            height: H as u32,
            fullscreen: false,
            min_size: None,
        }
    }

    fn update(&mut self, _msg: Self::Message, _needs_rebuild: &mut bool, _exit: &mut bool) {}
    fn tick(&mut self, _dt: f32, _needs_rebuild: &mut bool) {}

    fn display_list(&mut self, size: LogicalSize, scale: f64) -> Option<DisplayList> {
        cce_ui::scale::set_scale_factor(scale as f32);
        let (w, h) = (size.width as f32, size.height as f32);
        let mut pc = PaintCtx::new();
        pc.plate_spec(&PlateSpec {
            rect: Rect { x: 0.0, y: 0.0, width: w, height: h },
            material: Material::opaque([0.2, 0.2, 0.22, 1.0]),
            window_corners: (true, true, true, true),
            depth: cce_ui::layout::bevel_width(),
        });
        for i in 0..12 {
            let c = if i % 2 == 0 { [1.0, 1.0, 1.0, 1.0] } else { [0.0, 0.0, 0.0, 1.0] };
            pc.rounded_rect(Rect { x: i as f32 * COL, y: 0.0, width: COL, height: h }, 0.0, (false, false, false, false), c);
        }
        for (i, (k, radius)) in RECIPES.iter().enumerate() {
            let m = Material::opaque(TINT).with_frost(Frost::Frosted { compression: *k, refraction: 0.0, radius: *radius });
            pc.plate_spec(&PlateSpec { rect: plate_rect(i), material: m, window_corners: (false, false, false, false), depth: 6.0 });
        }
        Some(pc.finish())
    }

    fn handle_pointer_move(&mut self, _pos: LogicalPosition, _needs_rebuild: &mut bool) {}
    fn handle_mouse_input(&mut self, _b: MouseButton, _s: ElementState, _p: LogicalPosition, _n: &mut bool) -> Option<Self::Message> {
        None
    }
    fn handle_mouse_wheel(&mut self, _d: &MouseScrollDelta, _p: LogicalPosition, _n: &mut bool) {}
    fn handle_key_input(&mut self, _e: &KeyEvent, _n: &mut bool) -> Option<Self::Message> {
        None
    }
}

fn main() {
    cce_ui::engine::run::<FrostPair>();
}
