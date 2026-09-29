// A textured quad in the 3D scene pass: `SceneImage`. The uniform block is
// scene3d.wgsl's own, slot for slot — image draws take their uniforms from
// the same per-frame buffer the mesh draws do — and the texture is a user
// image's descriptor set, the one the 2D pass draws it with.
struct Uniforms {
    mvp: mat4x4<f32>,
    window_size: vec2<f32>,
    window_radius: f32,
    corner_shape: f32,
    wire_tint: vec4<f32>,
    opacity: f32,
    is_wire: f32,
    prelit: f32,
}

@group(0) @binding(0) var<uniform> uniforms: Uniforms;
@group(1) @binding(0) var t_image: texture_2d<f32>;
@group(1) @binding(1) var s_image: sampler;

// scene3d.wgsl's window_corner_distance, kept in lockstep with it.
fn window_corner_distance(pos: vec2<f32>) -> f32 {
    let w = uniforms.window_size.x;
    let h = uniforms.window_size.y;
    let r = uniforms.window_radius;

    if (pos.x < 0.0 || pos.x > w || pos.y < 0.0 || pos.y > h) {
        return 1e5;
    }
    if (r <= 0.0) {
        return -1e5;
    }
    let q = abs(pos - vec2f(w * 0.5, h * 0.5)) - vec2f(w * 0.5 - r, h * 0.5 - r);
    if (q.x > 0.0 && q.y > 0.0) {
        let shape = uniforms.corner_shape;
        if (shape > 2.001) {
            let lp = max(pow(pow(q.x, shape) + pow(q.y, shape), 1.0 / shape), 1e-4);
            let g = vec2f(pow(q.x / lp, shape - 1.0), pow(q.y / lp, shape - 1.0));
            return (lp - r) / max(length(g), 1e-4);
        }
        return length(q) - r;
    }
    return -1e5;
}

struct VertexOutput {
    @builtin(position) position: vec4f,
    @location(0) uv: vec2f,
};

@vertex
fn vs_main(
    @location(0) position: vec3f,
    @location(1) uv: vec2f,
) -> VertexOutput {
    var out: VertexOutput;
    out.position = uniforms.mvp * vec4f(position, 1.0);
    out.uv = uv;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4f {
    // Sampled ahead of any discard: implicit derivatives are undefined after
    // non-uniform control flow.
    let texel = textureSample(t_image, s_image, in.uv);
    let cov = 1.0 - smoothstep(-0.5, 0.5, window_corner_distance(in.position.xy));
    let alpha = texel.a * cov * uniforms.opacity;
    // The pipeline writes depth, and a texel that shows nothing must not
    // hide what is drawn behind it afterwards.
    if (alpha <= 0.002) {
        discard;
    }
    // Unlit: a picture carries its own light.
    return vec4f(texel.rgb, alpha);
}
