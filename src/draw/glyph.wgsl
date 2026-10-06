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
    @location(3) clip_extents: vec2f,
}

@vertex
fn vs_main(
    @location(0) position: vec2f,
    @location(1) uv: vec2f,
    @location(2) color: vec4f,
    @location(3) clip_circle: vec3f,
    @location(4) clip_extents: vec2f,
) -> VertexOutput {
    var out: VertexOutput;
    out.clip_position = vec4f(position, 0.0, 1.0);
    out.uv = uv;
    out.color = color;
    out.clip_circle = clip_circle;
    out.clip_extents = clip_extents;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4f {
    // Rounded-rect SDF clip in framebuffer px: center clip_circle.xy, corner radius
    // clip_circle.z, inner-box half-size clip_extents. Zero extents degenerate to the
    // plain circle clip (the circular network pane's curved rim labels); a non-zero
    // box clips plate children at the plate's rounded corners.
    if (in.clip_circle.z > 0.0) {
        let q = abs(in.clip_position.xy - in.clip_circle.xy) - in.clip_extents;
        let d = length(max(q, vec2f(0.0))) - in.clip_circle.z;
        if (d > 0.0) {
            discard;
        }
    }
    return in.color * textureSample(t_atlas, s_atlas, in.uv);
}
