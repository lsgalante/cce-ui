// A lit, textured mesh in the 3D scene pass: `LitDraw` (draw/lit.rs). The
// uniform block's head (mvp, window size, corner radius and shape) is
// scene3d.wgsl's, so the window-corner cut below is the same curve; the
// rest is the draw's material and the scene's light. Group 1 is a user
// image's descriptor set, as `SceneImage` binds it: the base-colour texture,
// or a 1x1 white one for an untextured draw (a set must be bound either way).
//
// Shading is the metallic-roughness model at its plainest: Lambert diffuse
// and a GGX / Smith / Schlick specular for each of two directional lights,
// plus a sky/ground hemisphere that is both the diffuse ambient and what a
// metal sees reflected. The diffuse term is albedo x light x N.L with no
// 1/pi, the scale the host's baked lighting has always used, and the
// specular is scaled to match, so a matte lit draw looks like a baked one.
struct Uniforms {
    mvp: mat4x4<f32>,
    window_size: vec2<f32>,
    window_radius: f32,
    corner_shape: f32,
    eye: vec4<f32>,
    // rgb the base colour, a the draw's opacity.
    base: vec4<f32>,
    // metallic, roughness, textured (0/1), unused.
    surface: vec4<f32>,
    key_toward: vec4<f32>,
    key_color: vec4<f32>,
    fill_toward: vec4<f32>,
    fill_color: vec4<f32>,
    sky: vec4<f32>,
    ground: vec4<f32>,
}

@group(0) @binding(0) var<uniform> u: Uniforms;
@group(1) @binding(0) var t_base: texture_2d<f32>;
@group(1) @binding(1) var s_base: sampler;

const PI: f32 = 3.14159265;

// scene3d.wgsl's window_corner_distance, kept in lockstep with it.
fn window_corner_distance(pos: vec2<f32>) -> f32 {
    let w = u.window_size.x;
    let h = u.window_size.y;
    let r = u.window_radius;

    if (pos.x < 0.0 || pos.x > w || pos.y < 0.0 || pos.y > h) {
        return 1e5;
    }
    if (r <= 0.0) {
        return -1e5;
    }
    let q = abs(pos - vec2f(w * 0.5, h * 0.5)) - vec2f(w * 0.5 - r, h * 0.5 - r);
    if (q.x > 0.0 && q.y > 0.0) {
        let shape = u.corner_shape;
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
    @location(0) world: vec3f,
    @location(1) normal: vec3f,
    @location(2) uv: vec2f,
    @location(3) color: vec3f,
};

@vertex
fn vs_main(
    @location(0) position: vec3f,
    @location(1) normal: vec3f,
    @location(2) uv: vec2f,
    @location(3) color: vec3f,
) -> VertexOutput {
    var out: VertexOutput;
    out.position = u.mvp * vec4f(position, 1.0);
    out.world = position;
    out.normal = normal;
    out.uv = uv;
    out.color = color;
    return out;
}

// The GGX distribution, for alpha = roughness^2.
fn ggx(n_h: f32, a: f32) -> f32 {
    let a2 = a * a;
    let d = n_h * n_h * (a2 - 1.0) + 1.0;
    return a2 / (PI * d * d);
}

// Smith-Schlick geometry term for both directions.
fn smith(n_v: f32, n_l: f32, roughness: f32) -> f32 {
    let k = (roughness + 1.0) * (roughness + 1.0) / 8.0;
    return (n_v / (n_v * (1.0 - k) + k)) * (n_l / (n_l * (1.0 - k) + k));
}

fn schlick(f0: vec3f, cos_theta: f32) -> vec3f {
    return f0 + (vec3f(1.0) - f0) * pow(1.0 - cos_theta, 5.0);
}

// One directional light's contribution.
fn shade(n: vec3f, v: vec3f, l: vec3f, color: vec3f, diffuse: vec3f, f0: vec3f, roughness: f32) -> vec3f {
    let n_l = dot(n, l);
    if (n_l <= 0.0) {
        return vec3f(0.0);
    }
    let h = normalize(l + v);
    let n_v = max(dot(n, v), 1e-4);
    let n_h = max(dot(n, h), 0.0);
    let v_h = max(dot(v, h), 0.0);
    let a = roughness * roughness;
    let spec = ggx(n_h, a) * smith(n_v, n_l, roughness) * schlick(f0, v_h) / (4.0 * n_v * n_l + 1e-4);
    // x pi: the diffuse carries no 1/pi, so the specular is scaled alike.
    return color * n_l * (diffuse + spec * PI);
}

@fragment
fn fs_main(in: VertexOutput, @builtin(front_facing) front: bool) -> @location(0) vec4f {
    // The sampler clamps, so the coordinates are wrapped here (glTF repeats
    // by default), with the UNWRAPPED coordinates' gradients: fract jumps
    // where the wrap falls, and its own gradients there would pick the
    // smallest mip and draw a seam. Sampled ahead of any discard: implicit
    // derivatives are undefined after non-uniform control flow.
    let gx = dpdx(in.uv);
    let gy = dpdy(in.uv);
    let texel = textureSampleGrad(t_base, s_base, fract(in.uv), gx, gy);
    let textured = u.surface.z > 0.5;

    var albedo = u.base.rgb * in.color;
    var alpha = u.base.a;
    if (textured) {
        albedo = albedo * texel.rgb;
        alpha = alpha * texel.a;
    }
    let cov = 1.0 - smoothstep(-0.5, 0.5, window_corner_distance(in.position.xy));
    alpha = alpha * cov;
    if (alpha <= 0.002) {
        discard;
    }

    // Both sides lit: a face seen from behind is shaded by its back.
    var n = normalize(in.normal);
    if (!front) {
        n = -n;
    }
    let v = normalize(u.eye.xyz - in.world);
    let metallic = u.surface.x;
    let roughness = u.surface.y;
    let diffuse = albedo * (1.0 - metallic);
    let f0 = mix(vec3f(0.04), albedo, metallic);

    let ambient = mix(u.ground.rgb, u.sky.rgb, 0.5 + 0.5 * n.y);
    let r = reflect(-v, n);
    let seen = mix(u.ground.rgb, u.sky.rgb, 0.5 + 0.5 * r.y);
    let n_v = max(dot(n, v), 1e-4);
    // Schlick with roughness: a rough surface reflects less at grazing.
    let fr = f0 + (max(vec3f(1.0 - roughness), f0) - f0) * pow(1.0 - n_v, 5.0);

    var color = diffuse * ambient + seen * fr;
    color += shade(n, v, u.key_toward.xyz, u.key_color.rgb, diffuse, f0, roughness);
    color += shade(n, v, u.fill_toward.xyz, u.fill_color.rgb, diffuse, f0, roughness);
    return vec4f(color, alpha);
}
