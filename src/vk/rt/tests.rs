use super::*;

#[test]
fn test_rt_shaders_compile() {
    // naga parse + validate + SPIR-V write for both tiers; panics on failure.
    let tier1 = compile_wgsl(&crate::draw::shaders::rt_bvh_source());
    assert!(!tier1.is_empty());
    let tier2 = compile_wgsl_ray_query(&format!("{}\n{}", crate::draw::shaders::RT_COMMON, crate::draw::shaders::RT_QUERY));
    assert!(!tier2.is_empty());
    let denoise = compile_wgsl(crate::draw::shaders::RT_DENOISE);
    assert!(!denoise.is_empty());
}

/// An image in the traced scene, end to end: a quad half red and half
/// clear, in front of a green wall. The red half shows red, the clear
/// half shows the wall behind it, and beside the quad is the wall too.
/// Run with: cargo test --lib vk::rt -- --ignored
#[test]
#[ignore = "requires a Vulkan device"]
fn test_offscreen_renders_an_image() {
    let mut off = RtOffscreen::new();
    // 2x1: a red texel, a clear one.
    let pixels = [255u8, 0, 0, 255, 0, 0, 0, 0];
    let wall = |p0, p1, p2| RtTriangle { p0, p1, p2, material: 0 };
    off.set_scene_with_image(
        &[
            wall([-9.0, -9.0, -1.0], [9.0, -9.0, -1.0], [9.0, 9.0, -1.0]),
            wall([-9.0, -9.0, -1.0], [9.0, 9.0, -1.0], [-9.0, 9.0, -1.0]),
        ],
        &[RtMaterial { albedo: [0.1, 0.9, 0.1], emission: [0.0; 3] }],
        Some(RtImagePixels {
            pixels: &pixels,
            width: 2,
            height: 1,
            corners: [[-1.0, 0.5, 0.0], [1.0, 0.5, 0.0], [1.0, -0.5, 0.0], [-1.0, -0.5, 0.0]],
            opacity: 1.0,
        }),
    );
    let proj = glam::Mat4::perspective_rh(0.9, 1.0, 0.1, 100.0);
    let view = glam::Mat4::look_at_rh(
        glam::Vec3::new(0.0, 0.0, 3.0),
        glam::Vec3::ZERO,
        glam::Vec3::Y,
    );
    let camera = RtCamera { inv_mvp: (proj * view).inverse().to_cols_array_2d() };
    let (w, h) = (64u32, 64u32);
    let px = off.render(camera, w, h, 64);
    let at = |x: u32, y: u32| {
        let i = ((y * w + x) * 4) as usize;
        (px[i] as i32, px[i + 1] as i32, px[i + 2] as i32)
    };
    // The quad spans x in -1..1 of a view about 2.9 wide at z = 0: the
    // pane's columns 10 to 54, and rows 21 to 43.
    // Unlit: the texel's own colour, whatever the sky is doing.
    let (r, g, b) = at(16, 32);
    assert!(r >= 250 && g <= 5 && b <= 5, "the image's red half is not its red: {r} {g} {b}");
    let (r, g, _) = at(48, 32);
    assert!(g > r + 40, "the image's clear half hides the wall: r={r} g={g}");
    let (r, g, _) = at(32, 6);
    assert!(g > r + 40, "beside the image is not the wall: r={r} g={g}");

    // Without its image the scene is the wall alone.
    off.set_scene(
        &[wall([-9.0, -9.0, -1.0], [9.0, -9.0, -1.0], [9.0, 9.0, -1.0])],
        &[RtMaterial { albedo: [0.1, 0.9, 0.1], emission: [0.0; 3] }],
    );
    let px = off.render(camera, w, h, 16);
    let i = ((32 * w + 40) * 4) as usize;
    assert!(px[i + 1] > px[i], "the image outlived its scene");
}

/// End-to-end GPU test — needs a Vulkan device, so ignored by default.
/// Run with: cargo test --lib vk::rt -- --ignored
#[test]
#[ignore = "requires a Vulkan device"]
fn test_offscreen_render_smoke() {
    let mut off = RtOffscreen::new();
    // A red triangle filling the view center, camera looking down -Z.
    off.set_scene(
        &[RtTriangle {
            p0: [-1.0, -1.0, 0.0],
            p1: [1.0, -1.0, 0.0],
            p2: [0.0, 1.5, 0.0],
            material: 0,
        }],
        &[RtMaterial { albedo: [0.9, 0.1, 0.1], emission: [0.0; 3] }],
    );
    let proj = glam::Mat4::perspective_rh(0.9, 1.0, 0.1, 100.0);
    let view = glam::Mat4::look_at_rh(
        glam::Vec3::new(0.0, 0.0, 3.0),
        glam::Vec3::ZERO,
        glam::Vec3::Y,
    );
    let camera = RtCamera { inv_mvp: (proj * view).inverse().to_cols_array_2d() };
    let (w, h) = (64u32, 64u32);
    let px = off.render(camera, w, h, 16);
    assert_eq!(px.len(), (w * h * 4) as usize);
    // Center pixel hits the triangle: red-dominant. Corner pixel is sky:
    // blue >= red. Alpha opaque everywhere.
    let at = |x: u32, y: u32| {
        let i = ((y * w + x) * 4) as usize;
        (px[i], px[i + 1], px[i + 2], px[i + 3])
    };
    let (cr, _cg, cb, ca) = at(w / 2, h / 2);
    assert!(ca == 255, "alpha not opaque: {ca}");
    assert!(cr > cb, "center not red-dominant: r={cr} b={cb}");
    let (sr, _sg, sb, _sa) = at(1, 1);
    assert!(sb >= sr, "corner sky not blue-ish: r={sr} b={sb}");
}

/// A background colour is what a camera ray that meets nothing shows —
/// every sample's, not only the one that writes the denoiser's features
/// (a render takes several per dispatch) — while the sky still lights
/// what is hit; and None is the sky again.
#[test]
#[ignore = "requires a Vulkan device"]
fn test_offscreen_background_is_what_a_miss_shows() {
    let mut off = RtOffscreen::new();
    off.set_scene(
        &[RtTriangle { p0: [-1.0, -1.0, 0.0], p1: [1.0, -1.0, 0.0], p2: [0.0, 1.5, 0.0], material: 0 }],
        &[RtMaterial { albedo: [0.8, 0.8, 0.8], emission: [0.0; 3] }],
    );
    let proj = glam::Mat4::perspective_rh(0.9, 1.0, 0.1, 100.0);
    let view = glam::Mat4::look_at_rh(glam::Vec3::new(0.0, 0.0, 3.0), glam::Vec3::ZERO, glam::Vec3::Y);
    let camera = RtCamera { inv_mvp: (proj * view).inverse().to_cols_array_2d() };
    let (w, h) = (32u32, 32u32);
    let render = |off: &mut RtOffscreen| {
        let px = off.render(camera, w, h, 16);
        let at = |x: u32, y: u32| {
            let i = ((y * w + x) * 4) as usize;
            [px[i], px[i + 1], px[i + 2]]
        };
        (at(1, 1), at(w / 2, h / 2))
    };
    let (sky, lit) = render(&mut off);

    off.set_background(Some([0.0, 0.0, 0.0]));
    let (corner, centre) = render(&mut off);
    assert_eq!(corner, [0, 0, 0], "the corner is the backdrop, in every sample");
    assert!(centre.iter().all(|&c| c > 60), "the sky still lights the triangle: {centre:?}");
    assert!(centre.iter().zip(lit).all(|(&a, b)| a.abs_diff(b) < 24), "and lights it as before: {centre:?} against {lit:?}");

    off.set_background(None);
    assert_eq!(render(&mut off).0, sky, "None is the sky");
}

/// The environment is the scene's light: a black one leaves a grey
/// triangle black, a sun in front of it lights it and the same sun
/// behind it does not, and the sky's colours are what a miss shows.
#[test]
#[ignore = "requires a Vulkan device"]
fn test_offscreen_environment_lights_the_scene() {
    let mut off = RtOffscreen::new();
    off.set_scene(
        &[RtTriangle { p0: [-1.0, -1.0, 0.0], p1: [1.0, -1.0, 0.0], p2: [0.0, 1.5, 0.0], material: 0 }],
        &[RtMaterial { albedo: [0.8, 0.8, 0.8], emission: [0.0; 3] }],
    );
    let proj = glam::Mat4::perspective_rh(0.9, 1.0, 0.1, 100.0);
    let view = glam::Mat4::look_at_rh(glam::Vec3::new(0.0, 0.0, 3.0), glam::Vec3::ZERO, glam::Vec3::Y);
    let camera = RtCamera { inv_mvp: (proj * view).inverse().to_cols_array_2d() };
    let (w, h) = (32u32, 32u32);
    let mut render = |env: RtEnvironment| {
        off.set_environment(env);
        let px = off.render(camera, w, h, 32);
        let at = |x: u32, y: u32| {
            let i = ((y * w + x) * 4) as usize;
            [px[i], px[i + 1], px[i + 2]]
        };
        (at(1, 1), at(w / 2, h / 2))
    };
    let dark = RtEnvironment { sun_direction: [0.0, 0.0, 1.0], sun_color: [0.0; 3], sky_zenith: [0.0; 3], sky_nadir: [0.0; 3] };
    assert_eq!(render(dark), ([0, 0, 0], [0, 0, 0]), "no light, nothing seen");
    let front = render(RtEnvironment { sun_color: [40.0; 3], ..dark }).1;
    let behind = render(RtEnvironment { sun_direction: [0.0, 0.0, -1.0], sun_color: [40.0; 3], ..dark }).1;
    assert!(front[0] > 100, "a sun in front lights the face: {front:?}");
    assert!(behind[0] < front[0] / 4, "a sun behind it does not: {behind:?} against {front:?}");
    let (corner, _) = render(RtEnvironment { sky_zenith: [0.0, 1.0, 0.0], sky_nadir: [0.0, 1.0, 0.0], ..dark });
    assert!(corner[1] > 200 && corner[0] < 10, "a miss shows the sky's colour: {corner:?}");
}
