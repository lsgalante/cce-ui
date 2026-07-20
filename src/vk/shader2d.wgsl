// The toolkit's 2D pipeline shader — the union of the two wgpu-era dialects:
// the engine shader's wavy-blob effect (clip_circle.x == -999 sentinel) and the
// designer shader's window-corner rounding + circle clip + blur-behind branch
// (negative alpha samples the backdrop). Clients that don't use a feature pay
// nothing: radius 0 disables corner rounding, the backdrop is renderer-managed,
// and plain quads take the final `return in.color` path.

@group(0) @binding(0) var t_backdrop: texture_2d<f32>;
@group(0) @binding(1) var s_backdrop: sampler;

struct WindowInfo {
    window_size: vec2<f32>,
    corner_radius: f32,
    padding: f32,
}

@group(0) @binding(2) var<uniform> window_info: WindowInfo;

// One carve (recess) belonging to an SDF-lit plate: a rounded box subtracted
// from the plate's material. rect = center + half-extents, radii per-corner
// (both physical px; a wall the carve shares with the plate's edge is encoded
// by extending the box past the plate on that side). params = [transition
// width px, depth px, 0, 0].
struct PlateFeature {
    rect: vec4f,
    radii: vec4f,
    params: vec4f,
}
// Double-buffered by frame-in-flight: slot k's 64 entries belong to frame
// index k. The plate's push constants carry the absolute offset.
struct PlateFeatures {
    items: array<PlateFeature, 128>,
}
@group(0) @binding(3) var<uniform> plate_features: PlateFeatures;

fn is_outside_window_corners(pos: vec2<f32>) -> bool {
    let w = window_info.window_size.x;
    let h = window_info.window_size.y;
    let r = window_info.corner_radius;

    if (r <= 0.0) {
        return false;
    }
    // Top-left
    if (pos.x < r && pos.y < r) {
        let dx = pos.x - r;
        let dy = pos.y - r;
        return (dx * dx + dy * dy) > r * r;
    }
    // Top-right
    if (pos.x > w - r && pos.y < r) {
        let dx = pos.x - (w - r);
        let dy = pos.y - r;
        return (dx * dx + dy * dy) > r * r;
    }
    // Bottom-left
    if (pos.x < r && pos.y > h - r) {
        let dx = pos.x - r;
        let dy = pos.y - (h - r);
        return (dx * dx + dy * dy) > r * r;
    }
    // Bottom-right
    if (pos.x > w - r && pos.y > h - r) {
        let dx = pos.x - (w - r);
        let dy = pos.y - (h - r);
        return (dx * dx + dy * dy) > r * r;
    }
    // Boundary check
    if (pos.x < 0.0 || pos.x > w || pos.y < 0.0 || pos.y > h) {
        return true;
    }
    return false;
}

// Per-batch push constants (112 bytes). The first two vec4s are the rounded-rect
// clip: rect0 = [cx, cy, bx, by] (center + SDF half-extents), rect1 = [corner
// radius, enabled flag, plate mode, plate corner shape]. When plate mode is
// nonzero the batch is an SDF-lit plate (1 = raised plate, 2 = recess overlay)
// and the p_* block describes it; the corner shape exponent selects circular
// (2) vs superellipse (> 2) plate corners — see plate_sdf_grad. Physical
// pixels, like clip_position.
struct RRectClip {
    rect0: vec4f,
    rect1: vec4f,
    // Plate SDF box: center + half-extents. May extend past the drawn cover
    // quad — that is how a recess suppresses a wall (the edge lies outside the
    // covered pixels, so its shading never lands).
    p_rect: vec4f,
    // Per-corner radii [tl, tr, br, bl].
    p_radii: vec4f,
    // xyz = unit vector toward the light (screen space, +z out of the screen),
    // w = bevel roll width in px.
    p_light: vec4f,
    // [shading strength, specular strength, shininess, curvature/AO strength].
    p_mat: vec4f,
    // Mode 1 (raised plate): xy = [offset, count] into plate_features — the
    // carves CSG'd out of this plate's material.
    // Mode 2 (free recess overlay): the host-plate box (center + half-extents)
    // the carve fades out against — a wall flush with the host's edge dies
    // across the host's perimeter roll; far-away sides sit at ±1e5 (no fade).
    p_host: vec4f,
}
var<push_constant> rrect_clip: RRectClip;

const TAU: f32 = 6.28318530718;
// Ambient floor of the plate lighting model: the fraction of illumination that
// arrives from everywhere rather than from the directional light. Keeps shadow
// walls readable instead of crushing to black.
const PLATE_AMBIENT: f32 = 0.55;
// Amplitude of the bright crest line hugging a raised plate's silhouette — the
// ambient-catching convex rim that makes glass read as glass.
const PLATE_CREST: f32 = 0.25;
// Recess depth as a fraction of the roll width (a recess is visually shallower
// than a raised plate's full quarter-round).
const RECESS_DEPTH: f32 = 0.6;

// Signed distance and gradient of the plate's rounded box at p, as
// (grad.x, grad.y, distance). Analytic — no dpdx/dpdy — so the clip discards
// above the plate branch cannot poison derivative quads, and corners need no
// special casing: the gradient swings continuously around each arc.
//
// rect1.w is the corner shape exponent: 2 = circular arcs; > 2 swaps them for
// superellipse (Lp-norm) corners — Apple-style continuous curvature, where
// curvature ramps smoothly to zero at the edge join instead of jumping from
// 1/r, so the lit roll's highlight sweeps a corner without a G2 kink. The Lp
// gradient is not unit length, so both the direction and the distance carry a
// first-order |∇| correction — exact on the boundary, and well within a shade
// step over the roll's few-px band.
fn rr_sdf_grad(p: vec2f, prect: vec4f, pradii: vec4f) -> vec3f {
    let c = p - prect.xy;
    let side = select(pradii.xw, pradii.yz, c.x > 0.0);
    let r = select(side.x, side.y, c.y > 0.0);
    let q = abs(c) - prect.zw + vec2f(r, r);
    let s = vec2f(select(-1.0, 1.0, c.x >= 0.0), select(-1.0, 1.0, c.y >= 0.0));
    if (q.x > 0.0 && q.y > 0.0) {
        let shape = rrect_clip.rect1.w;
        if (shape > 2.001) {
            let lp = max(pow(pow(q.x, shape) + pow(q.y, shape), 1.0 / shape), 1e-4);
            let g = vec2f(pow(q.x / lp, shape - 1.0), pow(q.y / lp, shape - 1.0));
            let gm = max(length(g), 1e-4);
            return vec3f(s * g / gm, (lp - r) / gm);
        }
        let len = max(length(q), 1e-4);
        return vec3f(s * q / len, len - r);
    }
    if (q.x > q.y) {
        return vec3f(s.x, 0.0, q.x - r);
    }
    return vec3f(0.0, s.y, q.y - r);
}

// Specular of a roll at tilt `slope` whose outward horizontal facing is along
// `g`: the profile alignment (how close the roll's tilt is to the half-vector's
// tilt) powered by shininess, times a gentle azimuthal falloff, minus the flat
// face's baseline so the face contributes zero. Deliberately DECOUPLED rather
// than Blinn-Phong's pow(dot(n, hv), s): coupled, a straight edge's normal can
// never fully reach the half-vector (it tilts in one plane only) while a corner
// diagonal's can, so the power function crushes edge lines relative to corner
// glints and the meeting fattens into a blob that ignores the corner arc.
// Decoupled, the band keeps constant inset, width, and peak intensity as it
// sweeps a corner — the highlight follows the silhouette.
// `sv` is the surface's slope vector — the horizontal part of the unnormalized
// normal (-∇height, 1): its magnitude is the tilt, its direction the facing.
fn roll_spec(sv: vec2f) -> f32 {
    let m = length(sv);
    if (m < 1e-5) {
        return 0.0;
    }
    let hv = normalize(rrect_clip.p_light.xyz + vec3f(0.0, 0.0, 1.0));
    let shininess = rrect_clip.p_mat.z;
    let facing = sv / m;
    let cos_t = inverseSqrt(1.0 + m * m);
    let sin_t = m * cos_t;
    let hxy = length(hv.xy);
    let prof = cos_t * hv.z + sin_t * hxy; // cos(tilt - half-vector tilt)
    let az = clamp(dot(facing, hv.xy) / max(hxy, 1e-4), 0.0, 1.0);
    return rrect_clip.p_mat.y * max(pow(prof, shininess) - pow(hv.z, shininess), 0.0) * az * az;
}

// Slope of the raised roll's height profile at f (0 at the face join, 1 at the
// silhouette). Circular (shape 2): a quarter-round h = sqrt(1 - f²) — tangent-
// continuous with the face but with a curvature JUMP at the join (1/t → 0), the
// profile-space twin of a circular plan corner. shape > 2 swaps in the matching
// superellipse quadrant h = (1 - f^n)^(1/n): its curvature ramps to zero at the
// join, so the roll's shading fades into the face instead of ending on a line.
// The slope has the closed form (f/h)^(n-1), which IS the circular formula at
// n = 2 — the same one-exponent generalization as the plan corners.
fn roll_slope(f: f32) -> f32 {
    let shape = rrect_clip.rect1.w;
    if (shape > 2.001) {
        let h = pow(max(1.0 - pow(f, shape), 1e-4), 1.0 / shape);
        return pow(f / h, shape - 1.0);
    }
    return f / sqrt(max(1.0 - f * f, 1e-4));
}

// Slope of a carve's transition profile (0 on the surrounding plateau → 1 on
// the carve floor) at v in [0, 1] across the wall — always >= 0, zero at both
// ends. The profile is smoothstep normally; smootherstep (zero SECOND
// derivative at both plateaus) under a continuous-curvature corner_shape —
// the step's analog of the superellipse roll.
fn carve_slope(v: f32) -> f32 {
    if (rrect_clip.rect1.w > 2.001) {
        let w = v * (1.0 - v);
        return 30.0 * w * w;
    }
    return 6.0 * v * (1.0 - v);
}

// Per-pixel lighting of a plate. The plate is one composite height field:
// the host's rolled-edge surface minus every carve's profile, with the carve
// depth measured RELATIVE to the local surface (a deboss/etch, not a flat
// milling plane — a flat tool would swallow the perimeter roll wherever a
// band overlaps it, deleting the plate's own edge shading there). Heights
// subtract, so slope vectors ADD: the pixel's normal comes from the summed
// analytic slopes of every feature over it, and the junction where a carve's
// wall crosses the plate's perimeter roll is the smooth composite of both
// tilts, ending in the rim notch a real groove leaves. One lighting
// evaluation per pixel — features never blend in color space. Shading is
// expressed relative to the flat face (shade ratio 1.0, specular delta 0.0)
// so the face keeps exactly the app's chosen color.
fn plate_shade(frag: vec2f, vcol: vec4f) -> vec4f {
    let gd = rr_sdf_grad(frag, rrect_clip.p_rect, rrect_clip.p_radii);
    let d = -gd.z; // positive inside the plate, in px
    let t = max(rrect_clip.p_light.w, 0.001);
    let l = rrect_clip.p_light.xyz;
    let strength = rrect_clip.p_mat.x;
    let flat_shade = PLATE_AMBIENT + (1.0 - PLATE_AMBIENT) * l.z;

    if (rrect_clip.rect1.z < 1.5) {
        let aa = clamp(d + 0.5, 0.0, 1.0); // 1px silhouette anti-aliasing
        if (aa <= 0.0) {
            discard;
        }
        var base = vcol;
        if (vcol.a < 0.0) {
            base = resolve_blur(frag, vcol);
        }
        let u = clamp(d / t, 0.0, 1.0);
        let f = 1.0 - u;
        // Host roll slope vector: vertical at the silhouette, flat where the
        // roll meets the face — then every carve's slope adds to it, and its
        // shoulder/fillet ambient term joins the roll's crest.
        var sv = gd.xy * roll_slope(f);
        var extra = PLATE_CREST * f * f * f;
        let f_off = u32(rrect_clip.p_host.x);
        let f_cnt = u32(rrect_clip.p_host.y);
        for (var i = 0u; i < f_cnt; i = i + 1u) {
            let feat = plate_features.items[f_off + i];
            let fg = rr_sdf_grad(frag, feat.rect, feat.radii);
            let ft = max(feat.params.x, 0.001);
            let v = clamp(-fg.z / ft + 0.5, 0.0, 1.0);
            if (v <= 0.0) {
                continue;
            }
            // params.y (depth) is signed: positive carves down, negative
            // raises a boss. The slope vector follows automatically; the
            // shoulder/fillet ambient term flips with it (a boss's convex
            // shoulder is at the top of its wall, not the bottom).
            sv += -(feat.params.y / ft) * carve_slope(v) * fg.xy;
            extra += rrect_clip.p_mat.w * sin(v * TAU) * sign(feat.params.y);
        }
        let n = normalize(vec3f(sv, 1.0));
        let diff = PLATE_AMBIENT + (1.0 - PLATE_AMBIENT) * max(dot(n, l), 0.0);
        let shade = 1.0 + (diff / flat_shade - 1.0 + extra) * strength;
        let spec = roll_spec(sv);
        return vec4f(base.rgb * shade + vec3f(spec * strength), abs(base.a) * aa);
    }

    // Free-floating recess, boss, or ridge (one not grouped into a host plate —
    // e.g. in a widget's own paint): an overlay over whatever is painted
    // beneath — no fill, no silhouette. Junction behavior here is the heuristic
    // host-box fade; grouped features get the exact CSG above.
    // Mode 2 = recess (interior one step DOWN), mode 3 = boss (interior one
    // step UP) — the same wall with the height sign flipped. Mode 4 = ridge: a
    // raised bump straddling the boundary, both sides at the base level — ONE
    // profile evaluation, so its crest carries a single specular/shoulder term
    // instead of a boss+recess double-stack.
    // All profiles straddle the boundary (span [-t/2, t/2]). Darkening is exact
    // multiplicative shading (black at alpha 1 - shade); brightening is a
    // translucent white screen.
    let u = clamp(d / t + 0.5, 0.0, 1.0);
    var slope = 0.0;
    var curv = 0.0;
    if (rrect_clip.rect1.z > 3.5) {
        // Ridge bump: the carve profile mirrored about the boundary (rising
        // outer half, falling inner half), amplitude halved so the wall tilt
        // matches a step's despite the doubled profile rate.
        let w = clamp(select(2.0 * u, 2.0 - 2.0 * u, u > 0.5), 0.0, 1.0);
        let rising = select(-1.0, 1.0, u <= 0.5);
        slope = rising * 0.5 * RECESS_DEPTH * 2.0 * carve_slope(w);
        // Each half-wall is a boss wall: concave fillet at its base, convex
        // shoulder toward the crest — and ZERO at the plateaus and crest, so
        // flat ground composites to exactly nothing (a constant term here
        // tints the whole cover quad).
        curv = -rrect_clip.p_mat.w * sin(w * TAU);
    } else {
        let dir = select(-1.0, 1.0, rrect_clip.rect1.z > 2.5);
        // The profile slope is carve_slope's family: smoothstep-derived
        // normally, smootherstep (zero second derivative at the plateaus)
        // under a continuous-curvature corner_shape — shading eases in and out
        // instead of starting on a line.
        slope = dir * RECESS_DEPTH * carve_slope(u);
        // Curvature: the convex shoulder catches ambient light, the concave
        // fillet self-occludes — on the outer half for a recess, inner for a
        // boss.
        curv = -dir * rrect_clip.p_mat.w * sin(u * TAU);
    }
    let sv = gd.xy * slope;
    let n = normalize(vec3f(sv, 1.0));
    let diff = PLATE_AMBIENT + (1.0 - PLATE_AMBIENT) * max(dot(n, l), 0.0);
    let spec = roll_spec(sv);
    // Fade the carve out across the host plate's perimeter roll (see p_host).
    let hb = rrect_clip.p_host;
    let host_d = min(hb.z - abs(frag.x - hb.x), hb.w - abs(frag.y - hb.y));
    let att = clamp(host_d / t, 0.0, 1.0);
    let v = (diff / flat_shade - 1.0 + curv + spec) * strength * att;
    if (v >= 0.0) {
        return vec4f(1.0, 1.0, 1.0, min(v, 1.0));
    }
    return vec4f(0.0, 0.0, 0.0, min(-v, 1.0));
}

struct VertexOutput {
    @builtin(position) clip_position: vec4f,
    @location(0) color: vec4f,
    @location(1) ndc_position: vec2f,
    @location(2) clip_circle: vec3f,
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
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4f {
    // Wavy-blob effect (engine shader.wgsl): the -999 sentinel renders a
    // rippled, fading disc in NDC space.
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

    if (is_outside_window_corners(in.clip_position.xy)) {
        discard;
    }
    if (in.clip_circle.z > 0.0) {
        let dx = in.clip_position.x - in.clip_circle.x;
        let dy = in.clip_position.y - in.clip_circle.y;
        if (dx * dx + dy * dy > in.clip_circle.z * in.clip_circle.z) {
            discard;
        }
    }
    // Rounded-rect clip (per-batch): SDF of the round-cornered box; outside discards.
    if (rrect_clip.rect1.y > 0.5) {
        let q = abs(in.clip_position.xy - rrect_clip.rect0.xy) - rrect_clip.rect0.zw;
        let d = length(max(q, vec2f(0.0))) - rrect_clip.rect1.x;
        if (d > 0.0) {
            discard;
        }
    }

    // SDF-lit plate batch (mode in the push constants; see plate_shade).
    if (rrect_clip.rect1.z > 0.5) {
        return plate_shade(in.clip_position.xy, in.color);
    }

    // Blur-behind plate: negative alpha mixes the (blurred) backdrop with the
    // plate color at |alpha| opacity.
    if (in.color.a < 0.0) {
        return resolve_blur(in.clip_position.xy, in.color);
    }

    return in.color;
}

// Blur-behind resolve for a negative-alpha plate color: the (blurred) backdrop
// mixed with the plate color at |alpha| opacity.
fn resolve_blur(pos: vec2f, color: vec4f) -> vec4f {
    let tex_size = vec2f(textureDimensions(t_backdrop));
    let clean_backdrop = textureSample(t_backdrop, s_backdrop, pos / tex_size);

    var blurred = vec4f(0.0);
    var total_weight = 0.0;

    // 7x7 Gaussian blur kernel
    for (var x = -3.0; x <= 3.0; x += 1.0) {
        for (var y = -3.0; y <= 3.0; y += 1.0) {
            let offset = vec2f(x, y) * 2.0; // sample every 2 pixels for a wider blur
            let sample_uv = (pos + offset) / tex_size;
            let weight = exp(-(x*x + y*y) / (2.0 * 2.0 * 2.0));
            blurred += textureSample(t_backdrop, s_backdrop, sample_uv) * weight;
            total_weight += weight;
        }
    }

    let backdrop_color = blurred / total_weight;
    let opacity = -color.a;
    let plate_color = vec4f(color.rgb, 1.0);
    let blurred_plate = mix(backdrop_color, plate_color, opacity);
    return mix(clean_backdrop, blurred_plate, opacity);
}
