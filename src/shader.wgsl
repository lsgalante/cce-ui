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
    var final_color = in.color;
    if (in.is_background > 0.999) {
        // Feather the background quad edges
        let dist_x = 1.0 - abs(in.ndc_position.x);
        let dist_y = 1.0 - abs(in.ndc_position.y);
        let min_dist = min(dist_x, dist_y);

        let feather = 0.15; // Size of the feathering zone (15% of the half-width/height)
        let fade = smoothstep(0.0, 1.0, min_dist / feather);
        final_color.a = final_color.a * fade;
    }
    if (abs(in.color.a - 0.699) < 0.001) {
        // Frosted glass effect
        // Generate pseudo-random high-frequency noise based on pixel position
        let noise = fract(sin(dot(in.clip_position.xy, vec2f(12.9898, 78.233))) * 43758.5453);
        let grain = (noise - 0.5) * 0.08;
        
        // Add a subtle diagonal light sheen
        let sheen = sin((in.clip_position.x + in.clip_position.y) * 0.012) * 0.018;
        
        final_color = vec4f(
            clamp(in.color.r + grain + sheen, 0.0, 1.0),
            clamp(in.color.g + grain + sheen, 0.0, 1.0),
            clamp(in.color.b + grain + sheen, 0.0, 1.0),
            0.699
        );
    }
    return final_color;
}
