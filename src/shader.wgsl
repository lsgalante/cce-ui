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
    if (in.clip_circle.x == -999.0) {
        let y = length(vec2f(in.ndc_position.x, in.ndc_position.y));
        let x = atan2(in.ndc_position.y, in.ndc_position.x);

        // Wavy boundary radius with 7 lobes
        let R_theta = 0.60 + 0.06 * sin(7.0 * x);

        // Radial density: 1.0 at center, fading out to 0.0 at R_theta
        let density = 1.0 - smoothstep(R_theta - 0.25, R_theta, y);

        // Sine wave effect driven by the x value (distance around the circle)
        let sin_effect = sin(7.0 * x);

        // Normalized radius from 0.0 (center) to 1.0 (boundary)
        let r_normalized = clamp(y / R_theta, 0.0, 1.0);

        let gray = in.color.xyz;
        
        // Scale the ripple amplitude by the normalized radius to fade it out at the center
        let alpha = clamp(density * (1.0 - r_normalized * 0.25 * (1.0 - sin_effect)), 0.0, 1.0);
        
        if (y > R_theta + 0.02) {
            discard;
        }
        
        let final_alpha = alpha * (1.0 - smoothstep(R_theta - 0.02, R_theta + 0.02, y)) * in.color.w;
        return vec4f(gray, final_alpha);
    }

    if (in.clip_circle.z > 0.0) {
        let dx = in.clip_position.x - in.clip_circle.x;
        let dy = in.clip_position.y - in.clip_circle.y;
        if (dx * dx + dy * dy > in.clip_circle.z * in.clip_circle.z) {
            discard;
        }
    }
    return in.color;
}
