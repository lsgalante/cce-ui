struct VertexOutput {
    @builtin(position) clip_position: vec4f,
    @location(0) color: vec4f,
    @location(1) ndc_position: vec2f,
    @location(2) @interpolate(flat) is_background: f32,
}

@vertex
fn vs_main(
    @location(0) position: vec2f,
    @location(1) color: vec4f,
) -> VertexOutput {
    var out: VertexOutput;
    out.clip_position = vec4f(position, 0.0, 1.0);
    out.color = color;
    out.ndc_position = position;

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
    var final_color = in.color;
    if (in.is_background > 0.5) {
        // Feather the background quad edges
        let dist_x = 1.0 - abs(in.ndc_position.x);
        let dist_y = 1.0 - abs(in.ndc_position.y);
        let min_dist = min(dist_x, dist_y);

        let feather = 0.15; // Size of the feathering zone (15% of the half-width/height)
        let fade = smoothstep(0.0, 1.0, min_dist / feather);
        final_color.a = final_color.a * fade;
    }
    return final_color;
}
