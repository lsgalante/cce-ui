//! The 3D probe's scene: one app whose frame is a 3D pane under a strip of
//! UI, staged through the portable `Application::init_3d` / `stage_3d`, so
//! the same code runs on the Vulkan renderer (`probe3d_native`) and the
//! WebGPU one (`probe3d_web`) and the two can be compared pixel for pixel.
//!
//! It reaches every path the scene pass has: the screen-space background
//! quad (`SceneDraw::screen_space`), flat-shaded fills (the derivative normal),
//! a prelit fill, a fill carrying a wire overlay (the depth-biased fill and
//! the line pipeline, wires tinted), a see-through translucent fill and the
//! wires riding it, an image standing in the scene before the translucent
//! draw, an INSTANCED draw (one white cube drawn for a row of coloured
//! instances), a light the host sets, and a frosted plate over the pane whose
//! blur samples the backdrop the scene left. And one regression: a small
//! sphere modelled in its own units around z = 9.99 and scaled into place,
//! which the old in-band background sentinel (any vertex within 0.01 of
//! z = 9.99) tore into spikes across the pane.

use cce_ui::engine::{
    AppSender, Application, LogicalPosition, LogicalSize, MeshId, RtCamera, RtImage, RtMaterial, RtTriangle, SceneDraw,
    SceneImage, Stage3D, Vertex3D, WindowSettings,
};
use cce_ui::scene::layout::Rect;
use cce_ui::scene::paint::{DisplayList, PaintCtx, PlateSpec};
use cce_ui::scene::{Frost, Material};
use cce_ui::widget::{ElementState, KeyEvent, MouseButton, MouseScrollDelta};
use glam::{Mat4, Vec3};

pub const W: u32 = 1280;
pub const H: u32 = 800;
/// The UI strip's width; the 3D pane is the rest of the window.
const STRIP: f32 = 300.0;

/// The traced probe's sample count: frames staged before it stops.
pub const TRACE_FRAMES: u32 = 8;

pub struct Probe3d<const TRACE: bool> {
    image: u32,
    meshes: Option<Meshes>,
    /// Traced frames staged so far.
    traced: u32,
}

pub type Raster = Probe3d<false>;
pub type Traced = Probe3d<true>;

/// The image's corners in the scene, as both passes place it.
const IMAGE_CORNERS: [[f32; 3]; 4] = [[-2.6, 1.6, -1.6], [-0.6, 1.6, -1.6], [-0.6, 0.35, -1.6], [-2.6, 0.35, -1.6]];

/// The far-z sphere, in its own units: centred ON z = 9.99, so its meridians
/// at longitude 0 and 180 degrees sit exactly on that plane.
const FAR_Z_CENTER: Vec3 = Vec3::new(0.0, 0.0, 9.99);
const FAR_Z_RADIUS: f32 = 4.0;

/// Where the far-z sphere's units land in the scene: a tenth of their size,
/// its centre at (-0.3, -0.3, 1.1), so it reads as a 0.4-radius ball in front
/// of the big sphere.
fn far_z_model() -> Mat4 {
    Mat4::from_translation(Vec3::new(-0.3, -0.3, 1.1)) * Mat4::from_scale(Vec3::splat(0.1)) * Mat4::from_translation(-FAR_Z_CENTER)
}

struct Meshes {
    background: MeshId,
    cube: MeshId,
    prelit: MeshId,
    sphere: MeshId,
    sphere_wires: MeshId,
    glass: MeshId,
    glass_wires: MeshId,
    /// A small white cube, and the row of instances it is drawn for.
    marker: MeshId,
    marker_instances: MeshId,
    /// The sphere spanning z = 9.99 in its own units (`far_z_model`).
    far_z: MeshId,
}

fn r(x: f32, y: f32, w: f32, h: f32) -> Rect {
    Rect { x, y, width: w, height: h }
}

fn v(p: Vec3, c: [f32; 3]) -> Vertex3D {
    Vertex3D { position: p.to_array(), color: c }
}

/// A box's triangles, counter-clockwise seen from outside, a colour a face.
fn cuboid(center: Vec3, half: Vec3, colors: [[f32; 3]; 6]) -> Vec<Vertex3D> {
    let c = |x: f32, y: f32, z: f32| center + half * Vec3::new(x, y, z);
    let faces = [
        [c(1., -1., -1.), c(1., 1., -1.), c(1., 1., 1.), c(1., -1., 1.)],     // +x
        [c(-1., -1., 1.), c(-1., 1., 1.), c(-1., 1., -1.), c(-1., -1., -1.)], // -x
        [c(-1., 1., -1.), c(-1., 1., 1.), c(1., 1., 1.), c(1., 1., -1.)],     // +y
        [c(-1., -1., 1.), c(-1., -1., -1.), c(1., -1., -1.), c(1., -1., 1.)], // -y
        [c(-1., -1., 1.), c(1., -1., 1.), c(1., 1., 1.), c(-1., 1., 1.)],     // +z
        [c(1., -1., -1.), c(-1., -1., -1.), c(-1., 1., -1.), c(1., 1., -1.)], // -z
    ];
    let mut out = Vec::new();
    for (f, q) in faces.iter().enumerate() {
        for i in [0, 1, 2, 0, 2, 3] {
            out.push(v(q[i], colors[f]));
        }
    }
    out
}

/// A box's twelve edges as vertex pairs.
fn cuboid_edges(center: Vec3, half: Vec3, color: [f32; 3]) -> Vec<Vertex3D> {
    let mut out = Vec::new();
    for a in 0..8u32 {
        for b in a + 1..8 {
            if (a ^ b).count_ones() == 1 {
                for k in [a, b] {
                    let s = |bit: u32| if k & bit != 0 { 1.0 } else { -1.0 };
                    out.push(v(center + half * Vec3::new(s(1), s(2), s(4)), color));
                }
            }
        }
    }
    out
}

/// A UV sphere's triangles (outward, counter-clockwise) and its latitude and
/// longitude lines as vertex pairs.
fn sphere(center: Vec3, radius: f32, stacks: u32, slices: u32) -> (Vec<Vertex3D>, Vec<Vertex3D>) {
    let p = |i: u32, j: u32| {
        let th = std::f32::consts::PI * i as f32 / stacks as f32;
        let ph = std::f32::consts::TAU * j as f32 / slices as f32;
        center + radius * Vec3::new(th.sin() * ph.cos(), th.cos(), th.sin() * ph.sin())
    };
    let color = |q: Vec3| {
        let n = (q - center) / radius;
        [0.35 + 0.3 * n.x, 0.45 + 0.25 * n.y, 0.65 + 0.2 * n.z]
    };
    let mut tris = Vec::new();
    let mut lines = Vec::new();
    for i in 0..stacks {
        for j in 0..slices {
            let (a, b, c, d) = (p(i, j), p(i + 1, j), p(i + 1, j + 1), p(i, j + 1));
            for q in [a, c, b, a, d, c] {
                tris.push(v(q, color(q)));
            }
            for q in [a, b, a, d] {
                lines.push(v(q, color(q)));
            }
        }
    }
    (tris, lines)
}

/// The traced scene: the raster meshes' triangles, a material each.
fn traced_scene() -> (Vec<RtTriangle>, Vec<RtMaterial>) {
    let mut tris = Vec::new();
    let mut mats = Vec::new();
    let mut add = |verts: Vec<Vertex3D>, albedo: [f32; 3], emission: [f32; 3]| {
        let m = mats.len() as u32;
        mats.push(RtMaterial { albedo, emission });
        for t in verts.as_chunks::<3>().0 {
            tris.push(RtTriangle { p0: t[0].position, p1: t[1].position, p2: t[2].position, material: m });
        }
    };
    add(cuboid(Vec3::new(0.0, -0.75, 0.0), Vec3::new(3.0, 0.05, 2.4), [[0.3; 3]; 6]), [0.55, 0.57, 0.55], [0.0; 3]);
    add(cuboid(Vec3::new(-1.6, 0.0, 0.2), Vec3::splat(0.6), [[0.0; 3]; 6]), [0.8, 0.35, 0.25], [0.0; 3]);
    add(sphere(Vec3::new(0.4, 0.3, -0.4), 0.9, 12, 20).0, [0.45, 0.55, 0.75], [0.0; 3]);
    add(cuboid(Vec3::new(1.5, 0.1, 1.2), Vec3::splat(0.55), [[0.0; 3]; 6]), [0.3, 0.7, 0.9], [0.4, 0.9, 1.1]);
    let model = far_z_model();
    let far_z = sphere(FAR_Z_CENTER, FAR_Z_RADIUS, 10, 16).0.into_iter().map(|p| v(model.transform_point3(Vec3::from(p.position)), p.color)).collect();
    add(far_z, [0.9, 0.75, 0.3], [0.0; 3]);
    (tris, mats)
}

impl<const TRACE: bool> Probe3d<TRACE> {
    fn camera(size: LogicalSize, scale: f64) -> (Mat4, (u32, u32, u32, u32)) {
        let s = scale as f32;
        let (pw, ph) = ((size.width - STRIP) * s, size.height * s);
        let proj = Mat4::perspective_rh(40f32.to_radians(), pw / ph, 0.1, 100.0);
        let view = Mat4::look_at_rh(Vec3::new(3.2, 2.4, 5.6), Vec3::new(0.0, 0.2, 0.0), Vec3::Y);
        // The scissor is the pane; the projection is of the pane, offset into
        // it by shifting NDC x (the pass draws in window space).
        let ndc_w = pw / (size.width * s);
        let shift = Mat4::from_translation(Vec3::new(1.0 - ndc_w, 0.0, 0.0)) * Mat4::from_scale(Vec3::new(ndc_w, 1.0, 1.0));
        (shift * proj * view, ((STRIP * s) as u32, 0, pw as u32, ph as u32))
    }

    /// The traced pane's camera: the pane's own projection, unshifted —
    /// the tracer's image is the pane.
    fn trace_camera(size: LogicalSize, scale: f64) -> RtCamera {
        let s = scale as f32;
        let (pw, ph) = ((size.width - STRIP) * s, size.height * s);
        let proj = Mat4::perspective_rh(40f32.to_radians(), pw / ph, 0.1, 100.0);
        let view = Mat4::look_at_rh(Vec3::new(3.2, 2.4, 5.6), Vec3::new(0.0, 0.2, 0.0), Vec3::Y);
        RtCamera { inv_mvp: (proj * view).inverse().to_cols_array_2d() }
    }
}

impl<const TRACE: bool> Application for Probe3d<TRACE> {
    type Message = ();

    fn create(_sender: AppSender<()>) -> Self {
        let (iw, ih) = (64u32, 40u32);
        let mut px = Vec::with_capacity((iw * ih * 4) as usize);
        for y in 0..ih {
            for x in 0..iw {
                let a = if (x / 8 + y / 8) % 2 == 0 { 255 } else { 120 };
                px.extend([(x * 255 / (iw - 1)) as u8, (y * 255 / (ih - 1)) as u8, 200, a]);
            }
        }
        Probe3d { image: cce_ui::draw::upload_rgba(px, iw, ih), meshes: None, traced: 0 }
    }

    fn settings(&self) -> WindowSettings {
        WindowSettings { title: "probe3d".into(), app_id: "cce-probe3d".into(), width: W, height: H, fullscreen: false, min_size: None }
    }

    fn load_system_fonts(&self) -> bool {
        true
    }

    fn display_list_text(&self) -> bool {
        true
    }

    fn init_3d(&mut self, stage: &mut dyn Stage3D) {
        // NDC corners: the draw is `screen_space`, so z is ignored.
        let bg = |x: f32, y: f32, c: [f32; 3]| Vertex3D { position: [x, y, 0.0], color: c };
        let (top, bottom) = ([0.10, 0.12, 0.20], [0.30, 0.26, 0.22]);
        let background = stage.create_mesh(&[
            bg(-1.0, -1.0, bottom),
            bg(1.0, -1.0, bottom),
            bg(1.0, 1.0, top),
            bg(-1.0, -1.0, bottom),
            bg(1.0, 1.0, top),
            bg(-1.0, 1.0, top),
        ]);
        let shades = [[0.8, 0.3, 0.2], [0.7, 0.35, 0.25], [0.9, 0.5, 0.3], [0.5, 0.2, 0.15], [0.85, 0.4, 0.3], [0.6, 0.25, 0.2]];
        let cube = stage.create_mesh(&cuboid(Vec3::new(-1.6, 0.0, 0.2), Vec3::splat(0.6), shades));
        let floor = [[0.30, 0.32, 0.30]; 6];
        let mut prelit_verts = cuboid(Vec3::new(0.0, -0.75, 0.0), Vec3::new(3.0, 0.05, 2.4), floor);
        for (i, p) in prelit_verts.iter_mut().enumerate() {
            let k = 0.7 + 0.3 * ((i / 6) as f32 / 5.0);
            p.color = [p.color[0] * k, p.color[1] * k, p.color[2] * k];
        }
        let prelit = stage.create_mesh(&prelit_verts);
        let mut far_z_verts = sphere(FAR_Z_CENTER, FAR_Z_RADIUS, 10, 16).0;
        for p in &mut far_z_verts {
            p.color = [0.9, 0.75, 0.3];
        }
        let far_z = stage.create_mesh(&far_z_verts);
        let (tris, lines) = sphere(Vec3::new(0.4, 0.3, -0.4), 0.9, 12, 20);
        let sphere = stage.create_mesh(&tris);
        let sphere_wires = stage.create_mesh(&lines);
        let glass_c = Vec3::new(1.5, 0.1, 1.2);
        let glass = stage.create_mesh(&cuboid(glass_c, Vec3::splat(0.55), [[0.3, 0.7, 0.9]; 6]));
        let glass_wires = stage.create_mesh(&cuboid_edges(glass_c, Vec3::splat(0.55), [0.9, 0.95, 1.0]));
        let marker = stage.create_mesh(&cuboid(Vec3::ZERO, Vec3::splat(0.12), [[1.0; 3]; 6]));
        let row: Vec<Vertex3D> = (0..8)
            .map(|i| {
                let t = i as f32 / 7.0;
                v(Vec3::new(-2.4 + 0.55 * i as f32, -0.58, 1.9), [0.9 - 0.6 * t, 0.4 + 0.4 * t, 0.3 + 0.6 * t])
            })
            .collect();
        let marker_instances = stage.create_mesh(&row);
        stage.set_scene_light([0.6, 0.7, 0.4]);
        self.meshes = Some(Meshes { background, cube, prelit, sphere, sphere_wires, glass, glass_wires, marker, marker_instances, far_z });
        if TRACE {
            let (tris, mats) = traced_scene();
            stage.set_rt_scene_with_image(&tris, &mats, Some(RtImage { image: self.image, corners: IMAGE_CORNERS, opacity: 0.9 }));
        }
    }

    fn stage_3d(&mut self, stage: &mut dyn Stage3D, size: LogicalSize, scale: f64) -> bool {
        if TRACE {
            // A sample a frame for TRACE_FRAMES frames, then the backdrop
            // keeps the result.
            if self.traced >= TRACE_FRAMES {
                return false;
            }
            let (_, pane) = Self::camera(size, scale);
            stage.stage_rt(pane, Self::trace_camera(size, scale));
            self.traced += 1;
            return self.traced < TRACE_FRAMES;
        }
        let Some(m) = &self.meshes else { return false };
        let (mvp, scissor) = Self::camera(size, scale);
        let mvp = mvp.to_cols_array_2d();
        let draw = |mesh| SceneDraw {
            mesh,
            mvp,
            wireframe: false,
            wire_tint: [0.0; 4],
            opacity: 1.0,
            line_width: 1.0,
            wire_base_width: 0.0,
            prelit: false,
            see_through: false,
            instances: None,
            screen_space: false,
        };
        let far_z_mvp = (Mat4::from_cols_array_2d(&mvp) * far_z_model()).to_cols_array_2d();
        let draws = vec![
            SceneDraw { screen_space: true, ..draw(m.background) },
            SceneDraw { prelit: true, ..draw(m.prelit) },
            draw(m.cube),
            SceneDraw { instances: Some(m.marker_instances), ..draw(m.marker) },
            SceneDraw { mvp: far_z_mvp, ..draw(m.far_z) },
            SceneDraw { wire_base_width: 1.0, ..draw(m.sphere) },
            SceneDraw { wireframe: true, wire_tint: [1.0, 1.0, 1.0, 0.6], ..draw(m.sphere_wires) },
            SceneDraw { see_through: true, opacity: 0.45, ..draw(m.glass) },
            SceneDraw { wireframe: true, see_through: true, ..draw(m.glass_wires) },
        ];
        stage.stage_scene(scissor, draws);
        stage.stage_scene_images(vec![SceneImage {
            image: self.image,
            corners: IMAGE_CORNERS,
            mvp,
            opacity: 0.9,
            before: 7,
        }]);
        false
    }

    fn update(&mut self, _: (), _: &mut bool, _: &mut bool) {}
    fn tick(&mut self, _: f32, _: &mut bool) {}

    fn display_list(&mut self, size: LogicalSize, _scale: f64) -> Option<DisplayList> {
        let (_, h) = (size.width, size.height);
        let mut pc = PaintCtx::new();
        // The strip: a pane plate the UI stands on. The 3D pane is left
        // unpainted, so the backdrop shows there.
        pc.plate_spec(&PlateSpec {
            rect: r(0.0, 0.0, STRIP, h),
            material: Material::pane(),
            window_corners: (true, false, false, true),
            depth: cce_ui::layout::bevel_width(),
        });
        pc.text_with("3D probe".to_string(), 24.0, 24.0, 20.0, [230, 230, 236], Some("DejaVu Sans".into()), None);
        // A frosted plate over the pane: its blur samples the backdrop.
        let m = Material::opaque([0.0015, 0.0015, 0.0024, 0.25])
            .with_frost(Frost::Frosted { compression: 0.6, refraction: 0.0, radius: Frost::DEFAULT_RADIUS });
        pc.plate_spec(&PlateSpec { rect: r(STRIP + 40.0, h - 170.0, 420.0, 130.0), material: m, window_corners: (false, false, false, false), depth: 8.0 });
        pc.text_with("frosted over the scene".to_string(), STRIP + 64.0, h - 120.0, 16.0, [240, 240, 246], Some("DejaVu Sans".into()), None);
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
