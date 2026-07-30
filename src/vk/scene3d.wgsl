struct Uniforms {
    mvp: mat4x4<f32>,
    window_size: vec2<f32>,
    window_radius: f32,
    // Corner-shape exponent shared with shader2d: circular arc at 2,
    // superellipse squircle above.
    corner_shape: f32,
    // rgb + mix: fragment color mixed toward .rgb by .a. Zero = vertex
    // colors untouched; a wireframe pass overlaid on its own filled mesh
    // sets it so the lines separate from the identical fill beneath.
    wire_tint: vec4<f32>,
}

@group(0) @binding(0) var<uniform> uniforms: Uniforms;

// Signed distance to the window's rounded silhouette — shader2d's
// window_corner_distance, kept in lockstep so the 3D scene fill cuts along the
// exact curve the 2D pass (and the plates' tessellated corners) use: positive
// outside the corner arcs and past the window bounds, large-negative on the
// straight edges (those keep their hard cut).
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
    @location(0) color: vec3f,
    // World-space position, for the flat-shading normal; `lit` is 0 on the
    // screen-space background quad (the z=9.99 sentinel), 1 on scene geometry.
    @location(1) world: vec3f,
    @location(2) lit: f32,
};

@vertex
fn vs_main(
    @location(0) position: vec3f,
    @location(1) color: vec3f,
) -> VertexOutput {
    var out: VertexOutput;
    if (abs(position.z - 9.99) < 0.01) {
        out.position = vec4f(position.xy, 0.9999, 1.0);
        out.lit = 0.0;
    } else {
        out.position = uniforms.mvp * vec4f(position, 1.0);
        out.lit = 1.0;
    }
    out.color = color;
    out.world = position;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4f {
    // ~1px feather along the squircle window corner (the pass clears to
    // transparent and blends with straight alpha, so partial coverage fades
    // the scene out exactly at the silhouette).
    let cov = 1.0 - smoothstep(-0.5, 0.5, window_corner_distance(in.position.xy));
    if (cov <= 0.0) {
        discard;
    }
    var rgb = mix(in.color, uniforms.wire_tint.rgb, uniforms.wire_tint.a);
    // Flat shading off a fixed WORLD light: the facet normal comes from the
    // screen-space derivatives of the world position, so every facet keeps a
    // brightness pinned to its world orientation. That anchoring is what makes
    // an orbit read as the camera moving around stationary geometry — an unlit
    // scene's only cues are the vertex colors, and any rotationally
    // self-similar surface (a UV sphere's lattice, especially under a
    // wireframe overlay whose fill occludes the back wires) reads as glued to
    // the camera without it. Two-sided so unculled back faces stay sane.
    if (in.lit > 0.5) {
        let n = normalize(cross(dpdx(in.world), dpdy(in.world)));
        // A strongly AZIMUTHAL light, wrap-shaded. A near-vertical light (or a
        // two-sided |dot|) yields a latitude-dominated / 180-degree-symmetric
        // brightness pattern — invariant under a yaw orbit, which reads as the
        // scene turning with the camera. The horizontal component pins the lit
        // side to a world azimuth the orbit visibly sweeps across; the wrap
        // term keeps a soft floor without |dot|'s ambiguity (the fill pass
        // culls to front faces, so the derivative normal's sign is stable).
        let l = normalize(vec3f(-0.55, 0.45, 0.7));
        let d = clamp(dot(n, l) * 0.5 + 0.5, 0.0, 1.0);
        rgb = rgb * (0.55 + 0.45 * d);
    }
    return vec4f(rgb, cov);
}
