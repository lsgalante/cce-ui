struct VertexOutput {
    @builtin(position) clip_position: vec4f,
    @location(0) color: vec4f,
    @location(1) ndc_position: vec2f,
    @location(2) is_background: f32,
    @location(3) clip_circle: vec3f,
}

@vertex
fn vs_main(
    @location(0) position: vec2f,
    @location(1) color: vec4f,
    @location(2) clip_circle: vec3f,
) -> VertexOutput {
    var out: VertexOutput;
    out.clip_position = vec4f(position, 0.0, 1.0);
    out.color = color;
    out.ndc_position = position;
    out.clip_circle = clip_circle;

    // Check if the vertex is exactly at the NDC boundaries (meaning it is the background quad)
    if (abs(position.x) > 0.999 && abs(position.y) > 0.999) {
        out.is_background = 1.0;
    } else {
        out.is_background = 0.0;
    }

    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4f {
    if (in.clip_circle.z > 0.0) {
        let dx = in.clip_position.x - in.clip_circle.x;
        let dy = in.clip_position.y - in.clip_circle.y;
        if (dx * dx + dy * dy > in.clip_circle.z * in.clip_circle.z) {
            discard;
        }
    }
    return in.color;
}
