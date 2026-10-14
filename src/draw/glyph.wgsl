// Glyph-atlas pipeline for the ash text stage. Mask glyphs are stored as
// white-with-alpha texels, color (emoji) glyphs as-is with a white vertex
// color — one multiply covers both.

@group(0) @binding(0) var t_atlas: texture_2d<f32>;
@group(0) @binding(1) var s_atlas: sampler;

struct VertexOutput {
    @builtin(position) clip_position: vec4f,
    @location(0) uv: vec2f,
    @location(1) color: vec4f,
    @location(2) clip_circle: vec3f,
    @location(3) clip_extents: vec3f,
}

@vertex
fn vs_main(
    @location(0) position: vec2f,
    @location(1) uv: vec2f,
    @location(2) color: vec4f,
    @location(3) clip_circle: vec3f,
    @location(4) clip_extents: vec3f,
) -> VertexOutput {
    var out: VertexOutput;
    out.clip_position = vec4f(position, 0.0, 1.0);
    out.uv = uv;
    out.color = color;
    out.clip_circle = clip_circle;
    out.clip_extents = clip_extents;
    return out;
}

// Signed distance to the rounded box: q is the point's offset past the inner
// box (abs(p - center) - half-size), r the corner radius, shape the corner
// exponent — 2 = circular arcs, > 2 superellipse corners, shader2d's
// rr_sdf_grad family at first order (exact on the boundary, which is all a
// 1px feather reads).
fn rrect_distance(q: vec2f, r: f32, shape: f32) -> f32 {
    if (q.x > 0.0 && q.y > 0.0) {
        if (shape > 2.001) {
            let lp = max(pow(pow(q.x, shape) + pow(q.y, shape), 1.0 / shape), 1e-4);
            let g = vec2f(pow(q.x / lp, shape - 1.0), pow(q.y / lp, shape - 1.0));
            return (lp - r) / max(length(g), 1e-4);
        }
        return length(q) - r;
    }
    return max(q.x, q.y) - r;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4f {
    // Sampled ahead of any discard: implicit derivatives are undefined after
    // non-uniform control flow.
    let texel = textureSample(t_atlas, s_atlas, in.uv);
    // Rounded-rect SDF clip in framebuffer px: center clip_circle.xy, corner radius
    // clip_circle.z, inner-box half-size clip_extents.xy, corner shape clip_extents.z.
    // Zero extents degenerate to the plain circle clip (the circular network pane's
    // curved rim labels); a non-zero box clips plate children and images at the
    // plate's rounded corners. A ~1px feather, as shader2d folds into its clips, so a
    // clipped image corner and a drawn plate corner share both curve and AA.
    var cov = 1.0;
    if (in.clip_circle.z > 0.0) {
        let q = abs(in.clip_position.xy - in.clip_circle.xy) - in.clip_extents.xy;
        cov = 1.0 - smoothstep(-0.5, 0.5, rrect_distance(q, in.clip_circle.z, in.clip_extents.z));
        if (cov <= 0.0) {
            discard;
        }
    }
    let c = in.color * texel;
    return vec4f(c.rgb, c.a * cov);
}
