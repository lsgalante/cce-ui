//! The renderer probe's scene: one app drawn by both renderers, natively in
//! the Wayland runner (`probe_native`) and in a browser through the WebGPU
//! renderer (`probe_web`), so the two can be compared pixel for pixel.
//!
//! It reaches every path a frame takes: the root plate and its roll, flat
//! fills, a checker under frosted plates in all three frost recipes (the
//! blur snapshot), raised / flush / flat control plates, fields in each form,
//! recesses, bevels, a sphere, grooves, circles and capped vectors, text in
//! two families at four sizes, and an uploaded image drawn at two sizes.

use cce_ui::engine::{AppSender, Application, LogicalPosition, LogicalSize, WindowSettings};
use cce_ui::scene::layout::Rect;
use cce_ui::scene::paint::{Cap, ControlPlate, DisplayList, Field, PaintCtx, PlateSpec, PlateStance};
use cce_ui::scene::{Frost, Material};
use cce_ui::widget::{ElementState, KeyEvent, MouseButton, MouseScrollDelta};

pub const W: u32 = 1280;
pub const H: u32 = 800;

pub struct ProbeApp {
    image: u32,
}

fn r(x: f32, y: f32, w: f32, h: f32) -> Rect {
    Rect { x, y, width: w, height: h }
}

const SANS: &str = "DejaVu Sans";
const MONO: &str = "DejaVu Sans Mono";

impl Application for ProbeApp {
    type Message = ();

    fn create(_sender: AppSender<()>) -> Self {
        cce_ui::scale::set_scale_factor(1.0);
        let (iw, ih) = (64u32, 40u32);
        let mut px = Vec::with_capacity((iw * ih * 4) as usize);
        for y in 0..ih {
            for x in 0..iw {
                px.extend([(x * 255 / (iw - 1)) as u8, (y * 255 / (ih - 1)) as u8, 160, 255]);
            }
        }
        ProbeApp { image: cce_ui::draw::upload_rgba(px, iw, ih) }
    }

    fn settings(&self) -> WindowSettings {
        WindowSettings { title: "probe".into(), app_id: "cce-probe".into(), width: W, height: H, fullscreen: false, min_size: None }
    }

    fn load_system_fonts(&self) -> bool {
        true
    }

    fn display_list_text(&self) -> bool {
        true
    }

    fn update(&mut self, _: (), _: &mut bool, _: &mut bool) {}
    fn tick(&mut self, _: f32, _: &mut bool) {}

    fn display_list(&mut self, size: LogicalSize, scale: f64) -> Option<DisplayList> {
        cce_ui::scale::set_scale_factor(scale as f32);
        let (w, h) = (size.width, size.height);
        let mut pc = PaintCtx::new();
        pc.root_plate(w, h);

        // A checker under three frosted plates: unfrosted-compression,
        // compressed, and clear (frost_pair's recipes).
        for i in 0..16 {
            let c = if i % 2 == 0 { [1.0, 1.0, 1.0, 1.0] } else { [0.02, 0.02, 0.03, 1.0] };
            pc.quad(r(30.0 + i as f32 * 30.0, 30.0, 30.0, 220.0), c);
        }
        let tint = [0.0015, 0.0015, 0.0024, 0.25];
        for (i, (k, radius)) in [(0.0, Frost::DEFAULT_RADIUS), (0.85, Frost::DEFAULT_RADIUS), (0.0, 0.0)].iter().enumerate() {
            let m = Material::opaque(tint).with_frost(Frost::Frosted { compression: *k, refraction: 0.0, radius: *radius });
            pc.plate_spec(&PlateSpec {
                rect: r(40.0 + i as f32 * 155.0, 60.0, 140.0, 160.0),
                material: m,
                window_corners: (false, false, false, false),
                depth: 6.0,
            });
        }

        // A pane plate with the control vocabulary on it.
        pc.plate_spec(&PlateSpec {
            rect: r(540.0, 30.0, 700.0, 330.0),
            material: Material::pane(),
            window_corners: (false, false, false, false),
            depth: cce_ui::layout::bevel_width(),
        });
        let face = Some(Material::opaque([0.18, 0.22, 0.32, 1.0]));
        pc.control_plate(&ControlPlate::control(r(570.0, 60.0, 140.0, 36.0), 8.0, PlateStance::Raised, face));
        pc.control_plate(&ControlPlate::control(r(730.0, 60.0, 140.0, 36.0), 8.0, PlateStance::Raised, None));
        pc.control_plate(&ControlPlate::control(r(890.0, 60.0, 140.0, 36.0), 8.0, PlateStance::Flush, None));
        pc.control_plate(&ControlPlate::control(r(1050.0, 60.0, 140.0, 36.0), 8.0, PlateStance::Flat, face));
        let radii = (8.0, 8.0, 8.0, 8.0);
        pc.field(&Field::well(r(570.0, 120.0, 300.0, 34.0), radii, 6.0));
        pc.field(&Field::ending_in_run(r(890.0, 120.0, 300.0, 34.0), radii, 6.0, 1150.0));
        pc.field(&Field::sliding_run(r(570.0, 180.0, 90.0, 34.0), (17.0, 17.0, 17.0, 17.0), 6.0, 45.0, 0.0));
        pc.field(&Field::sliding_run(r(680.0, 180.0, 90.0, 34.0), (17.0, 17.0, 17.0, 17.0), 6.0, 45.0, 1.0));
        pc.recess(r(800.0, 180.0, 120.0, 60.0), (12.0, 12.0, 12.0, 12.0), 8.0);
        pc.bevel(r(950.0, 180.0, 120.0, 60.0), (12.0, 12.0, 12.0, 12.0), &Material::opaque([0.35, 0.30, 0.42, 1.0]), 8.0);
        pc.sphere(1140.0, 210.0, 30.0, &Material::opaque([0.55, 0.25, 0.25, 1.0]));
        let host = r(540.0, 30.0, 700.0, 330.0);
        pc.groove((570.0, 280.0), (1210.0, 280.0), 4.0, 3.0, host);
        pc.groove((600.0, 300.0), (760.0, 340.0), 4.0, 3.0, host);

        // Flat geometry: circles and capped vectors.
        for (i, cap) in [Cap::Flat, Cap::Round, Cap::Arrow].into_iter().enumerate() {
            let y = 300.0 + i as f32 * 25.0;
            pc.vector(820.0, y, 1000.0, y + 10.0, 5.0, [0.9, 0.6, 0.2, 1.0], cap);
        }
        pc.circle(1080.0, 320.0, 22.0, [0.2, 0.7, 0.5, 1.0]);
        pc.circle(1150.0, 320.0, 22.0, [0.2, 0.5, 0.9, 0.6]);

        // Text, two families at four sizes.
        let mut y = 400.0;
        for (i, size) in [11.0, 14.0, 20.0, 28.0].into_iter().enumerate() {
            let family = if i % 2 == 0 { SANS } else { MONO };
            pc.text_with(
                format!("The quick brown fox jumps over the lazy dog — {size}px {family}"),
                40.0,
                y,
                size,
                [230, 230, 236],
                Some(family.to_string()),
                None,
            );
            y += size * 1.8;
        }

        // The image, at its size and scaled up.
        pc.image(self.image, r(40.0, 620.0, 64.0, 40.0), 1.0);
        pc.image(self.image, r(130.0, 600.0, 256.0, 160.0), 0.85);

        Some(pc.finish())
    }

    fn handle_pointer_move(&mut self, _: LogicalPosition, _: &mut bool) {}
    fn handle_mouse_input(&mut self, _: MouseButton, _: ElementState, _: LogicalPosition, _: &mut bool) -> Option<()> {
        None
    }
    fn handle_mouse_wheel(&mut self, _: &MouseScrollDelta, _: LogicalPosition, _: &mut bool) {}
    fn handle_key_input(&mut self, _: &KeyEvent, _: &mut bool) -> Option<()> {
        None
    }
}
