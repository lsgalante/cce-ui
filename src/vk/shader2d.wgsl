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
    // Corner-shape exponent shared with the plates and the rounded-rect clip:
    // circular arc at 2, superellipse squircle above.
    corner_shape: f32,
    // Custom bevel/carve profile (cce_ui::layout::set_bevel_profile_keys):
    // x nonzero enables it, y = live sample count in `profile`.
    profile_meta: vec4f,
    // Slope samples of the profile's height curve h(v) (v 0 = plateau, 1 =
    // carve floor / boss crest), sample i at v = (i + 0.5) / count, packed 4
    // per vec4. carve_slope reads these in place of its analytic smoothstep.
    profile: array<vec4f, 8>,
    // Custom EDGE profile for the plate perimeter roll
    // (cce_ui::layout::set_roll_profile_keys) — same encoding, read by
    // roll_slope in place of the analytic superellipse quadrant. The curve is
    // the roll's descent progress: 0 at the face join, 1 at the silhouette.
    roll_meta: vec4f,
    roll_profile: array<vec4f, 8>,
    // Pinned relief heights in physical px (cce_ui::layout::bevel_height /
    // roll_height): x = a carve's drop, y = the plate roll's rise. 0 = follow
    // the wall width — RECESS_DEPTH × width for a carve, a quarter-round of
    // radius width for the roll. Divided by the batch's own wall width
    // (p_light.w) they become the slope scale, so a pinned 0.5 mm drop is
    // the same geometry whatever wall it is cut with.
    relief_meta: vec4f,
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

// Signed distance to the window's rounded silhouette at pos: positive outside
// the corner arcs (and past the window bounds), large-negative elsewhere so the
// straight edges keep their exact hard cut at the buffer boundary. The corner
// family follows window_info.corner_shape — circular arc at 2, superellipse
// squircle above, with the Lp branch's first-order |∇| correction so a feather
// built on this distance keeps ~uniform width around the arc (the same
// construction as rr_sdf_grad and the tessellated plate corners).
fn window_corner_distance(pos: vec2<f32>) -> f32 {
    let w = window_info.window_size.x;
    let h = window_info.window_size.y;
    let r = window_info.corner_radius;

    if (pos.x < 0.0 || pos.x > w || pos.y < 0.0 || pos.y > h) {
        return 1e5;
    }
    if (r <= 0.0) {
        return -1e5;
    }
    let q = abs(pos - vec2f(w * 0.5, h * 0.5)) - vec2f(w * 0.5 - r, h * 0.5 - r);
    if (q.x > 0.0 && q.y > 0.0) {
        let shape = window_info.corner_shape;
        if (shape > 2.001) {
            let lp = max(pow(pow(q.x, shape) + pow(q.y, shape), 1.0 / shape), 1e-4);
            let g = vec2f(pow(q.x / lp, shape - 1.0), pow(q.y / lp, shape - 1.0));
            return (lp - r) / max(length(g), 1e-4);
        }
        return length(q) - r;
    }
    return -1e5;
}

// Per-batch push constants (112 bytes). The first two vec4s are the rounded-rect
// clip: rect0 = [cx, cy, bx, by] (center + SDF half-extents), rect1 = [corner
// radius, enabled flag, plate mode, corner shape]. When plate mode is
// nonzero the batch is an SDF-lit plate (1 = raised plate, 2 = recess overlay)
// and the p_* block describes it. The corner shape exponent selects circular
// (2) vs superellipse (> 2) corners for BOTH the clip SDF and the plate —
// see rr_sdf_grad; it is set whenever either consumer is live. Physical
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
    // carves CSG'd out of this plate's material; z = the plate's FROST
    // recipe, compression and refraction as 12-bit fixed point in one float
    // (hi·FROST_PACK_BASE + lo, each over FROST_PACK_MAX — see
    // material::Frost::pack); w = the blur kernel's sigma in physical px,
    // 0 = a clear plate (one clean sample). Unread on an opaque plate.
    // Mode 2 (free recess overlay): the host-plate box (center + half-extents)
    // the carve fades out against — a wall flush with the host's edge dies
    // across the host's perimeter roll; far-away sides sit at ±1e5 (no fade).
    p_host: vec4f,
    // RGB multiplies the lit roll's specular color — neutral white normally,
    // a highlight color on a marked (focused) plate. w unused.
    p_spec_tint: vec4f,
}
var<push_constant> rrect_clip: RRectClip;

// Plate modes, as carried in rect1.z (see PlatePush::mode). Compared by EQUALITY
// on a rounded int, never by range: the ranges these replaced were ordered, and
// the order was load-bearing without saying so — mode 8's branch had to precede
// the `> 5.5` fillet branch or the fillet arm would have swallowed it, taken 4
// off, and drawn every groove as a ridge. Equality makes a new mode inert
// wherever it is added rather than silently captured by a neighbour.
// Frost recipe packing — mirrored by `scene::material::Frost`, checked by
// its tests against this text.
const FROST_PACK_MAX: f32 = 4095.0;
const FROST_PACK_BASE: f32 = 4096.0;
// The kernel stride a frosted surface with NO recipe uses — the droplet
// (its push block is full) and a raw negative-alpha vertex from outside the
// display list: the panel's default kernel (Frost::DEFAULT_RADIUS at scale
// 2), which is exactly the fixed 5.5 px stride every frosted plate had
// before recipes were per plate.
const LEGACY_STRIDE: f32 = 5.5;

const MODE_NONE: i32 = 0;         // not a plate batch
const MODE_PLATE: i32 = 1;        // raised lit plate: fill + rolled perimeter + CSG carves
const MODE_RECESS: i32 = 2;       // free carve, interior one step DOWN
const MODE_BOSS: i32 = 3;         // free carve, interior one step UP
const MODE_RIDGE: i32 = 4;        // raised rim straddling the boundary
const MODE_SPHERE: i32 = 5;       // hemisphere-lit disc
const MODE_FILLET_DOWN: i32 = 6;  // concave inside-corner wall, recessed
const MODE_FILLET_UP: i32 = 7;    // concave inside-corner wall, raised
const MODE_GROOVE: i32 = 8;       // slab carve about an arbitrary line
const MODE_TROUGH: i32 = 9;       // sunken valley straddling the boundary
const MODE_DROPLET: i32 = 10;     // hanging water droplet clinging to the box top
const MODE_ROLL: i32 = 11;        // fill-less rolled perimeter, composited as an overlay
const MODE_DROPLET_SCRIM: i32 = 12; // flat feathered fill of the droplet silhouette
const MODE_LATTICE: i32 = 13;     // periodic well field: nearest-cell carve, one evaluation
const MODE_UNION: i32 = 14;       // union of feature boxes carved/raised as one wall
const MODE_GROUT: i32 = 15;       // flat colour outside a periodic field of rounded cells
// Fillet modes rejoin the shared free-carve path as their flat equivalents.
const FILLET_TO_STEP: i32 = 4;    // 6 -> RECESS, 7 -> BOSS

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
            // First-order |∇|-corrected distance: exact on the boundary and on
            // the axis/diagonal rays, but mid-arc it runs up to ~4% of the
            // depth low, so the roll band's contours drift off the true
            // parallels of the arc as the roll widens.
            let d0 = (lp - r) / gm;
            let dir = g / gm;
            // One re-evaluation at the projected near-boundary point tightens
            // the band to true parallels (error /5 to /10 over the lit part of
            // the roll). Trusted only near the boundary: past the roll the
            // projection approaches the Lp field's degenerate center and
            // diverges, so the step is clamped and the result blends back to
            // the plain first-order value — beyond 1.5 rolls the field is
            // bit-identical to the pre-refinement one (flat fill; only
            // crest/AO tails read it there).
            let roll = max(rrect_clip.p_light.w, 2.0);
            let step = clamp(d0, -roll, roll);
            let q1 = max(q - step * dir, vec2f(1e-4));
            let lp1 = max(pow(pow(q1.x, shape) + pow(q1.y, shape), 1.0 / shape), 1e-4);
            let g1 = vec2f(pow(q1.x / lp1, shape - 1.0), pow(q1.y / lp1, shape - 1.0));
            let gm1 = max(length(g1), 1e-4);
            let d1 = step + (lp1 - r) / gm1;
            let w = smoothstep(roll, roll * 1.5 + 2.0, abs(d0));
            let nrm = normalize(mix(g1 / gm1, dir, w));
            return vec3f(s * nrm, mix(d1, d0, w));
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

// roll_spec with the azimuth mask dropped: every edge shades as if it faced
// the light, so the glint the light-facing edges normally get sweeps the
// WHOLE silhouette at the same inset, width, and peak (the decoupled profile
// keeps those constant through corners by construction). The focused-plate
// treatment: the familiar specular line, accent-tinted, on all four sides.
fn roll_spec_wrap(sv: vec2f) -> f32 {
    let m = length(sv);
    if (m < 1e-5) {
        return 0.0;
    }
    let hv = normalize(rrect_clip.p_light.xyz + vec3f(0.0, 0.0, 1.0));
    let shininess = rrect_clip.p_mat.z;
    let cos_t = inverseSqrt(1.0 + m * m);
    let sin_t = m * cos_t;
    let hxy = length(hv.xy);
    let prof = cos_t * hv.z + sin_t * hxy;
    return rrect_clip.p_mat.y * max(pow(prof, shininess) - pow(hv.z, shininess), 0.0);
}

// Slope of the raised roll's height profile at f (0 at the face join, 1 at the
// silhouette). Circular (shape 2): a quarter-round h = sqrt(1 - f²) — tangent-
// continuous with the face but with a curvature JUMP at the join (1/t → 0), the
// profile-space twin of a circular plan corner. shape > 2 swaps in the matching
// superellipse quadrant h = (1 - f^n)^(1/n): its curvature ramps to zero at the
// join, so the roll's shading fades into the face instead of ending on a line.
// The slope has the closed form (f/h)^(n-1), which IS the circular formula at
// n = 2 — the same one-exponent generalization as the plan corners.
// The descent is truncated at ROLL_CUT of the quadrant: the roll shades as if
// the slab's rim were cut off partway down, so the profile ends on a bounded
// slope instead of plunging vertical at the silhouette (the full quadrant put
// nearly all of its drop in the outer third of the roll, reading as a hard
// dropoff line at the very edge).
const ROLL_CUT: f32 = 0.8;

fn roll_slope(f: f32) -> f32 {
    let t = max(rrect_clip.p_light.w, 0.001);
    let rr = select(1.0, window_info.relief_meta.y / t, window_info.relief_meta.y > 0.0);
    return roll_slope_unit(f) * rr;
}

// The unit-rise roll (a quarter-round of radius t, or the custom edge LUT);
// roll_slope scales it by the pinned rise.
fn roll_slope_unit(f: f32) -> f32 {
    // Custom edge profile: sample the uploaded ramp LUT. Face pixels saturate
    // at f = 0 (the roll band's interior end), so taper the slope to zero
    // there or every face pixel would inherit the curve's start slope; the
    // silhouette end keeps whatever slope the curve was drawn ending on.
    if (window_info.roll_meta.x > 0.5) {
        let n = window_info.roll_meta.y;
        let fcl = clamp(f, 0.0, 1.0);
        let x = clamp(fcl * n - 0.5, 0.0, n - 1.0);
        let i0 = u32(floor(x));
        let i1 = min(i0 + 1u, u32(n) - 1u);
        let fr = x - floor(x);
        let s0 = window_info.roll_profile[i0 >> 2u][i0 & 3u];
        let s1 = window_info.roll_profile[i1 >> 2u][i1 & 3u];
        let win = clamp(fcl * n * 0.667, 0.0, 1.0);
        return mix(s0, s1, fr) * win;
    }
    let shape = rrect_clip.rect1.w;
    let fc = f * ROLL_CUT;
    if (shape > 2.001) {
        let h = pow(max(1.0 - pow(fc, shape), 1e-4), 1.0 / shape);
        return pow(fc / h, shape - 1.0);
    }
    return fc / sqrt(max(1.0 - fc * fc, 1e-4));
}

// Slope of a carve's transition profile (0 on the surrounding plateau → 1 on
// the carve floor) at v in [0, 1] across the wall. With a custom profile
// installed (window_info.profile_meta.x), the slope comes from the uploaded
// ramp LUT — it may go negative (non-monotonic curves: rims, ogees) and its
// integral is the curve's net rise, not necessarily 1. Otherwise the analytic
// default: smoothstep normally, smootherstep (zero SECOND derivative at both
// plateaus) under a continuous-curvature corner_shape — the step's analog of
// the superellipse roll.
fn carve_slope(v: f32) -> f32 {
    if (window_info.profile_meta.x > 0.5) {
        let n = window_info.profile_meta.y;
        let vc = clamp(v, 0.0, 1.0);
        // Samples sit at v = (i + 0.5) / n; lerp between the two neighbors.
        let x = clamp(vc * n - 0.5, 0.0, n - 1.0);
        let i0 = u32(floor(x));
        let i1 = min(i0 + 1u, u32(n) - 1u);
        let fr = x - floor(x);
        let s0 = window_info.profile[i0 >> 2u][i0 & 3u];
        let s1 = window_info.profile[i1 >> 2u][i1 & 3u];
        // The curve describes ONLY the wall band; the surfaces on either side
        // are flat by definition, so taper to exactly zero at both ends
        // (~1.5 samples). Without this every face pixel (v saturates at 1
        // inside a feature) inherits the endpoint slope, and the SDF
        // gradient's nearest-edge regions facet the face into triangles.
        let win = clamp(min(vc, 1.0 - vc) * n * 0.667, 0.0, 1.0);
        return mix(s0, s1, fr) * win;
    }
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
    let l = rrect_clip.p_light.xyz;
    let strength = rrect_clip.p_mat.x;
    let flat_shade = PLATE_AMBIENT + (1.0 - PLATE_AMBIENT) * l.z;
    // The mode arrives as a float only because the push block is all f32.
    let mode = i32(round(rrect_clip.rect1.z));

    // MODE_SPHERE: a sphere-lit disc (the slider thumb). p_rect.xy is the center,
    // p_rect.z the radius, physical px. The disc is shaded as a hemisphere
    // under the same light/material as the plates — ambient floor, diffuse off
    // the sphere normal, the decoupled roll specular (its glint lands where
    // the surface tilt meets the half-vector, ~a third of the way out toward
    // the light) — and, like a plate face, the shade is expressed relative to
    // the flat face so the color at the lit center is exactly the app's.
    // MODE_GROUT: the vertex colour, flat, everywhere OUTSIDE a periodic
    // field of identical rounded cells — the grid lines of a graph whose
    // cells are whatever lies beneath showing through, corners included.
    // The same fold as MODE_LATTICE (p_rect = one cell's centre and
    // half-extents, p_host.xy = the period, p_radii = the corner radius),
    // but no lighting: coverage is the cell SDF's outside, 1px anti-aliased,
    // so the cells' superellipse corners are exact and the whole grid is one
    // draw. Flat strips could never paint the notch a rounded cell leaves at
    // each crossing.
    if (mode == MODE_GROUT) {
        let per = max(rrect_clip.p_host.xy, vec2f(1e-3));
        var gc = frag - rrect_clip.p_rect.xy;
        gc = gc - per * round(gc / per);
        let lg = rr_sdf_grad(gc, vec4f(0.0, 0.0, rrect_clip.p_rect.zw), rrect_clip.p_radii);
        let cov = clamp(lg.z + 0.5, 0.0, 1.0);
        if (cov <= 0.0) {
            discard;
        }
        return vec4f(vcol.rgb, vcol.a * cov);
    }

    if (mode == MODE_SPHERE) {
        let c = frag - rrect_clip.p_rect.xy;
        let r = max(rrect_clip.p_rect.z, 0.001);
        let dist = length(c);
        let aa = clamp(r - dist + 0.5, 0.0, 1.0); // 1px silhouette anti-aliasing
        if (aa <= 0.0) {
            discard;
        }
        let h = sqrt(max(r * r - dist * dist, 1e-3));
        let n = vec3f(c / r, h / r); // unit on the sphere's surface
        let diff = PLATE_AMBIENT + (1.0 - PLATE_AMBIENT) * max(dot(n, l), 0.0);
        let shade = 1.0 + (diff / flat_shade - 1.0) * strength;
        let spec = roll_spec(c / h);
        return vec4f(vcol.rgb * shade + rrect_clip.p_spec_tint.rgb * (spec * strength), vcol.a * aa);
    }

    // MODE_DROPLET: a hanging water droplet clinging to the box's TOP edge.
    // Field reinterpretation (the push block cannot grow):
    //   p_rect  = the droplet box, center + half-extents (like a plate);
    //   p_radii = [sag, belly radius, belly half-width, blend k] px;
    //   p_host  = [sheet bottom-corner radius px, edge clarity 0-1, dome
    //              amplitude, attach (top-corner) radius px];
    //   p_spec_tint = [core density, contact-shadow reach px, contact-shadow
    //     strength, bottom-bow edge rise px] — a droplet's glint is always
    //     white, so the tint RGB slots are free;
    //   p_mat.w = fresnel rim crest amplitude (droplets carve nothing, so the
    //             AO slot is free); p_light.w = shaded band width px.
    // The silhouette is the smooth union of a film SHEET attached to the top
    // edge (square top corners — the attach line; bottom lifted by sag) and a
    // BELLY capsule resting on the box bottom: the polynomial smin forms the
    // waist/neck a real drop's surface tension pulls in. Shading reuses the
    // plate vocabulary — roll_slope tilt over the band, ambient/diffuse,
    // decoupled roll specular — plus two water terms: a fresnel rim crest
    // (f³, like PLATE_CREST but tunable) and a thin-edge clarity falloff on
    // the tint alpha, so the (compositor- or resolve_blur-) frosted backdrop
    // shows through clearer at the rim.
    if (mode == MODE_DROPLET || mode == MODE_DROPLET_SCRIM) {
        let c = rrect_clip.p_rect.xy;
        let hx = rrect_clip.p_rect.z;
        let hy = rrect_clip.p_rect.w;
        let sag = rrect_clip.p_radii.x;
        let br = rrect_clip.p_radii.y;
        let bw = rrect_clip.p_radii.z;
        let k = max(rrect_clip.p_radii.w, 1.0);
        let sr = rrect_clip.p_host.x;
        let clarity = rrect_clip.p_host.y;
        let dome = rrect_clip.p_host.z;
        let ar = rrect_clip.p_host.w;

        // Sheet: bottom lifted by sag; top corners carry the attach radius —
        // the meniscus taper that curves the sides into the attach line (0 =
        // the square-shouldered clinging-pool look).
        let a_rect = vec4f(c.x, c.y - sag * 0.5, hx, hy - sag * 0.5);
        let ga = rr_sdf_grad(frag, a_rect, vec4f(ar, ar, sr, sr));
        var d = ga.z;
        var g = ga.xy;
        // Belly (radius > 0 only): a horizontal capsule resting on the box
        // bottom, joined by polynomial smooth union — one drop, smooth neck.
        // The gradient is the same weighted mix as the distance, renormalized.
        if (br > 0.5) {
            let b_rect = vec4f(c.x, c.y + hy - br, bw, br);
            let gb = rr_sdf_grad(frag, b_rect, vec4f(br));
            let hm = clamp(0.5 + 0.5 * (gb.z - ga.z) / k, 0.0, 1.0);
            d = mix(gb.z, ga.z, hm) - k * hm * (1.0 - hm);
            g = mix(gb.xy, ga.xy, hm);
        }
        // Bottom bow (p_spec_tint.w = edge rise, px): smooth-INTERSECT the
        // drop with a disc whose lowest point touches the drop's bottom
        // center — the bottom becomes one continuous circular arc, rising by
        // the given amount at x = ±hx. The radius follows from that fixed
        // rise (R = hx²/2·rise), so wide drops flatten toward the middle on
        // their own. smax = -smin(-a,-b): same polynomial blend, sign flipped.
        let bow = rrect_clip.p_spec_tint.w;
        if (bow > 0.25) {
            let bigr = hx * hx / (2.0 * bow);
            let cc = vec2f(c.x, c.y + hy - bigr);
            let pc = frag - cc;
            let dl = max(length(pc), 1e-3);
            let dc = dl - bigr;
            let gc = pc / dl;
            let hm2 = clamp(0.5 + 0.5 * (d - dc) / k, 0.0, 1.0);
            d = mix(dc, d, hm2) + k * hm2 * (1.0 - hm2);
            g = mix(gc, g, hm2);
        }
        g = normalize(g);

        let din = -d;
        let aa2 = clamp(din + 0.5, 0.0, 1.0);
        // MODE_DROPLET_SCRIM: the same silhouette, filled flat and feathered
        // inward — a vignette shaped exactly like the drop it sits in, for a
        // caller that needs a legible ground under text without a second lit
        // body. It shares this mode's SDF rather than approximating the shape
        // with a rounded rect, which is the whole point: the two can never
        // disagree about where the drop's edge is. p_light.w carries the
        // feather (px) instead of the shading band, which is only read below.
        if (mode == MODE_DROPLET_SCRIM) {
            if (aa2 <= 0.0) {
                discard;
            }
            let fth = max(rrect_clip.p_light.w, 0.001);
            let sa2 = vcol.a * clamp(din / fth, 0.0, 1.0) * aa2;
            if (sa2 <= 0.004) {
                discard;
            }
            return vec4f(vcol.rgb, sa2);
        }
        if (aa2 <= 0.0) {
            // Outside the silhouette: the contact shadow — a soft dark
            // falloff cast below the drop's lower arc (weighted by the
            // outward gradient's downward component, so the attach line and
            // sides stay clean). The cover quad overhangs the box by the
            // reach to give these fragments pixels to land on.
            let sh_reach = rrect_clip.p_spec_tint.y;
            let sh_amp = rrect_clip.p_spec_tint.z;
            if (sh_reach < 0.5 || sh_amp <= 0.0) {
                discard;
            }
            let down_sh = clamp(g.y, 0.0, 1.0);
            let sfall = 1.0 - clamp(d / sh_reach, 0.0, 1.0);
            let sa = sh_amp * sfall * sfall * down_sh;
            if (sa <= 0.004) {
                discard;
            }
            return vec4f(0.0, 0.0, 0.0, sa);
        }
        var base = vcol;
        if (vcol.a < 0.0) {
            // No recipe: every p_host slot is the drop's geometry. The
            // kernel default, no compression (what a drop always drew).
            base = resolve_blur(frag, vcol, vec2f(0.0), 0.0, 0.0, LEGACY_STRIDE);
        }
        let t2 = max(rrect_clip.p_light.w, 0.001);
        let u = clamp(din / t2, 0.0, 1.0);
        let f = 1.0 - u;
        // Continuous dome: the drop is a spherical-cap height field over the
        // silhouette — h = sqrt(2u - u²), vertical at the rim, flattening
        // toward the interior — so the normal varies over the ENTIRE body
        // and the diffuse rolls from lit shoulder to shaded belly instead of
        // reading as a flat face inside a shaded band (the old roll_slope
        // treatment, which only tilted the skirt).
        let hdome = sqrt(max(2.0 * u - u * u, 1e-4));
        let sv = g * ((1.0 - u) / hdome * dome);
        let n = normalize(vec3f(sv, 1.0));
        let diff = PLATE_AMBIENT + (1.0 - PLATE_AMBIENT) * max(dot(n, l), 0.0);
        // Rim crest weighted toward the BOTTOM edge (g.y > 0, y-down): a
        // hanging drop concentrates transmitted light into a caustic along
        // its lower arc, while the attach line stays quiet.
        let down = clamp(g.y, 0.0, 1.0);
        let extra = rrect_clip.p_mat.w * f * f * f * (0.3 + 1.2 * down * down);
        let shade = 1.0 + (diff / flat_shade - 1.0 + extra) * strength;
        let spec = roll_spec(sv);
        // Thin edges are clearer water; the deep interior densifies by the
        // core term (thickest water in the middle — the text's field).
        let body = mix(clarity, 1.0 + rrect_clip.p_spec_tint.x, u);
        return vec4f(
            base.rgb * shade + vec3f(spec * strength),
            min(abs(base.a) * body, 1.0) * aa2,
        );
    }

    let gd = rr_sdf_grad(frag, rrect_clip.p_rect, rrect_clip.p_radii);
    let d = -gd.z; // positive inside the plate, in px
    let t = max(rrect_clip.p_light.w, 0.001);

    if (mode == MODE_PLATE) {
        let aa = clamp(d + 0.5, 0.0, 1.0); // 1px silhouette anti-aliasing
        if (aa <= 0.0) {
            discard;
        }
        let u = clamp(d / t, 0.0, 1.0);
        let f = 1.0 - u;
        // Host roll slope vector: vertical at the silhouette, flat where the
        // roll meets the face — then every carve's slope adds to it, and its
        // shoulder/fillet ambient term joins the roll's crest.
        var sv = gd.xy * roll_slope(f);
        // The plate's OWN roll, kept apart from the carves added below: a
        // focused plate's accent ring traces this alone (see the tinted
        // branch), so the wells carved into it never wear the ring too.
        let sv_rim = sv;

        // Rim refraction, resolved BEFORE the shading below because the
        // backdrop it bends is `base`.
        //
        // The roll is a real surface with a real tilt — `sv_rim` IS that tilt
        // (the horizontal part of the unnormalized normal), already computed
        // for the specular. Displacing the backdrop sample along it is what a
        // curved edge does to what you see through it: the view compresses
        // toward the silhouette and the plate stops being a rectangle of haze
        // and starts being a slab with a thickness.
        //
        // Scaled by the roll width `t`, so a 12px bevel bends more than a 2px
        // one and the effect tracks the plate's own geometry rather than
        // drifting off it at another radius. The clarity ramp is f*f — the
        // clear window belongs to the outer third of the roll, and the face
        // must reach zero exactly or the whole plate unfrosts.
        // The plate's own recipe, from its push block (see RRectClip.p_host).
        let fz = rrect_clip.p_host.z;
        let fhi = floor(fz / FROST_PACK_BASE);
        let k_plate = clamp(fhi / FROST_PACK_MAX, 0.0, 1.0);
        let refr = clamp((fz - fhi * FROST_PACK_BASE) / FROST_PACK_MAX, 0.0, 1.0);
        let stride = rrect_clip.p_host.w * 0.5;
        var base = vcol;
        if (vcol.a < 0.0) {
            base = resolve_blur(frag, vcol, sv_rim * (refr * t), refr * f * f, k_plate, stride);
        }
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
        // p_spec_tint.w = 1 marks an accent-tinted plate (the focused-pane
        // treatment): the specular line WRAPS — the exact glint the
        // light-facing edges always carry runs the whole silhouette in the
        // accent color, same inset, width, and peak. Nothing else about the
        // plate's shading changes (accent-wash variants were tried and read
        // as painted frames). Neutral plates (w = 0) keep the directional
        // glint, byte-identical.
        let tw = rrect_clip.p_spec_tint.w;
        if (tw > 0.0) {
            // The wrap runs on the plate's own roll ONLY (sv_rim), never on
            // the carves: with the full slope every well carved into a
            // focused plate — each parameter control on the designer's
            // parameter pane — drew its own accent ring, reading as if every
            // control were focused alongside the pane.
            let spec = roll_spec_wrap(sv_rim);
            // A FILL-LESS tinted plate is a pure focus ring (the network
            // cursor): the wrapped glint alone, on the plate's own roll — so
            // the line traces the same superellipse silhouette, radius
            // family, and inset as every node and pane, which a
            // boundary-straddling carve band cannot (outward offsets of an
            // Lp corner round off).
            if (abs(base.a) < 0.004) {
                return vec4f(rrect_clip.p_spec_tint.rgb, spec * strength * aa);
            }
            // The carves keep the neutral directional glint an unfocused
            // plate gives them (white, as p_spec_tint.rgb is for w = 0);
            // sv_rim is zero on the face, so this is exactly their term.
            let carve_spec = roll_spec(sv - sv_rim);
            return vec4f(
                base.rgb * shade + rrect_clip.p_spec_tint.rgb * (spec * strength) + vec3f(carve_spec * strength),
                abs(base.a) * aa,
            );
        }
        let spec = roll_spec(sv);
        return vec4f(base.rgb * shade + rrect_clip.p_spec_tint.rgb * (spec * strength), abs(base.a) * aa);
    }

    if (mode == MODE_ROLL) {
        // Fill-less rolled perimeter: MODE_PLATE's roll — same profile, crest
        // and specular, spanning the full width INSIDE the silhouette — for a
        // window whose face is not a plate fill (the designer's full-bleed 3D
        // canvas). With no fill to shade into, it composites like the free
        // carves: darkening is a black multiply, brightening a translucent
        // white screen, over whatever is beneath. No CSG features: an overlay
        // owns no surface, so carves never group into it (the tessellator
        // never opens it as a host).
        let aa = clamp(d + 0.5, 0.0, 1.0);
        if (aa <= 0.0) {
            discard;
        }
        let u = clamp(d / t, 0.0, 1.0);
        let f = 1.0 - u;
        let sv = gd.xy * roll_slope(f);
        let extra = PLATE_CREST * f * f * f;
        let n = normalize(vec3f(sv, 1.0));
        let diff = PLATE_AMBIENT + (1.0 - PLATE_AMBIENT) * max(dot(n, l), 0.0);
        let spec = roll_spec(sv);
        let v = (diff / flat_shade - 1.0 + extra + spec) * strength * aa;
        if (v >= 0.0) {
            return vec4f(1.0, 1.0, 1.0, min(v, 1.0));
        }
        return vec4f(0.0, 0.0, 0.0, min(-v, 1.0));
    }

    // Free-floating recess, boss, or ridge (one not grouped into a host plate —
    // e.g. in a widget's own paint): an overlay over whatever is painted
    // beneath — no fill, no silhouette. Junction behavior here is the heuristic
    // host-box fade; grouped features get the exact CSG above.
    // MODE_RECESS = interior one step DOWN, MODE_BOSS = interior one step UP —
    // the same wall with the height sign flipped. MODE_RIDGE is a
    // raised bump straddling the boundary, both sides at the base level — ONE
    // profile evaluation, so its crest carries a single specular/shoulder term
    // instead of a boss+recess double-stack. MODE_TROUGH is that bump inverted
    // (a valley), for the same reason: it replaced the recess-ring+boss stack
    // `inset_plate` used to emit for every flush control in the DE.
    // All profiles straddle the boundary (span [-t/2, t/2]). Darkening is exact
    // multiplicative shading (black at alpha 1 - shade); brightening is a
    // translucent white screen.
    //
    // Concave fillet (MODE_FILLET_DOWN / MODE_FILLET_UP): the wall follows a
    // quarter ARC whose centre sits out in the pocket — the inside-corner
    // rounding the box SDF cannot express. p_rect.xy = centre, .z = radius;
    // p_radii.x = the wedge's start angle (quarter span, HARD-cut at the
    // tangent lines — the straight walls continue the profile exactly there).
    // Distance/gradient swap to radial; everything downstream is the shared
    // free-carve path via `eff` (minus FILLET_TO_STEP: 6→RECESS, 7→BOSS).
    var eff = mode;
    var fd = d;
    var fgd = gd.xy;
    var wedge = 1.0;
    // MODE_GROOVE: a SLAB carve — the band of half-width p_rect.z about the
    // line through p_rect.xy with unit normal p_radii.xy. Distance is |signed
    // distance to that line| minus the half-width, so ONE profile evaluation
    // yields both walls (the gradient flips sign across the centre line, tilting
    // them apart) and the groove costs a single specular term. The box SDF is
    // axis-aligned by construction; this is how a mark runs at an angle.
    // Rejoins the shared free-carve path as a recess (eff = 2).
    if (mode == MODE_GROOVE) {
        let nrm = rrect_clip.p_radii.xy;
        let c = frag - rrect_clip.p_rect.xy;
        let s = dot(c, nrm);
        fd = abs(s) - rrect_clip.p_rect.z;
        fgd = nrm * select(-1.0, 1.0, s >= 0.0);
        eff = MODE_RECESS;
    } else if (mode == MODE_LATTICE) {
        // MODE_LATTICE: a periodic field of identical rounded wells. Fold
        // the pixel into the period about one cell's centre (p_rect.xy;
        // period in p_host.xy) and take the box distance to THAT cell —
        // identical axis-aligned boxes centred in their period cells, so
        // the folded cell is always the nearest one and this is the exact
        // union distance of every well. One profile evaluation, so the
        // rails between cells and the diagonals at each crossing are true
        // mitres instead of stacked per-cell overlays. The wall runs from
        // the cell edge OUTWARD: floor at the edge, plateau one run out.
        //
        // The wall's outer edge is NOT the offset curve (every point one run
        // from the cell): that contour rounds each corner at radius + run,
        // and with a run of half a rail the crossings read as big sweeping
        // arcs while the cells themselves keep tight corners. A moulding
        // does not offset its corners, it mitres them — so the outer edge is
        // the cell box grown by the run with SHARP corners, and the wall is
        // the fraction of the way across the band between the two contours
        // (u = 1 at the cell edge, 0 at the outer contour). On the straight
        // rails that is exactly distance / run; around a corner the band
        // widens along the diagonal and four walls meet on the mitre lines.
        // Sharp, not the cell's radius, so that where the run is half the
        // rail the four outer boxes meet at a point and the crest lines run
        // continuously through the crossing as hips — a rounded outer corner
        // left a flat lozenge on top of every crossing. Lit by the cell's
        // gradient. The -t/2 recentres the shared path's boundary-straddling
        // band on [edge, edge + t].
        let per = max(rrect_clip.p_host.xy, vec2f(1e-3));
        var c = frag - rrect_clip.p_rect.xy;
        c = c - per * round(c / per);
        let lg = rr_sdf_grad(c, vec4f(0.0, 0.0, rrect_clip.p_rect.zw), rrect_clip.p_radii);
        let lo = rr_sdf_grad(c, vec4f(0.0, 0.0, rrect_clip.p_rect.zw + vec2f(t)), vec4f(0.0));
        let band = max(lg.z - lo.z, 1e-3);
        let frac = clamp(lg.z / band, 0.0, 1.0);
        fd = (0.5 - frac) * t;
        fgd = lg.xy;
        eff = MODE_RECESS;
    } else if (mode == MODE_UNION) {
        // MODE_UNION: the boxes in the feature run p_host.xy = [offset,
        // count] are one shape. Each box's wall is MITRED like the lattice's:
        // the band runs between the box shrunk by t/2 and the box grown by
        // t/2, both at the box's own corner radius, and the pixel's position
        // is its fraction across that band (an offset band would round the
        // outer corners at radius + t/2). The union takes the box the pixel
        // is deepest in — max over the run of the band coordinate, with that
        // box's gradient — so a box's wall vanishes inside another and the
        // outline is evaluated once. p_radii.x = 1 raises the union (boss)
        // instead of carving it.
        let u_off = u32(rrect_clip.p_host.x);
        let u_cnt = u32(rrect_clip.p_host.y);
        let hw = 0.5 * t;
        var best = -1e9;
        var bgrad = vec2f(0.0, -1.0);
        for (var i = 0u; i < u_cnt; i = i + 1u) {
            let feat = plate_features.items[u_off + i];
            let inner = vec4f(feat.rect.xy, max(feat.rect.zw - vec2f(hw), vec2f(0.5)));
            let outer = vec4f(feat.rect.xy, feat.rect.zw + vec2f(hw));
            let gi = rr_sdf_grad(frag, inner, feat.radii);
            let go = rr_sdf_grad(frag, outer, feat.radii);
            let band = max(gi.z - go.z, 1e-3);
            let fdi = (0.5 - clamp(gi.z / band, 0.0, 1.0)) * t;
            if (fdi > best) {
                best = fdi;
                bgrad = gi.xy;
            }
        }
        fd = best;
        fgd = bgrad;
        eff = select(MODE_RECESS, MODE_BOSS, rrect_clip.p_radii.x > 0.5);
    } else if (mode == MODE_FILLET_DOWN || mode == MODE_FILLET_UP) {
        eff = mode - FILLET_TO_STEP;
        let c = frag - rrect_clip.p_rect.xy;
        let dist = max(length(c), 1e-4);
        fd = dist - rrect_clip.p_rect.z;
        fgd = -c / dist;
        let a0 = rrect_clip.p_radii.x;
        let ang = atan2(c.y, c.x);
        let rel = ang - a0 - floor((ang - a0) / TAU) * TAU;
        wedge = select(0.0, 1.0, rel <= 1.5707964);
    }
    let u = clamp(fd / t + 0.5, 0.0, 1.0);
    // Drop over run: the pinned height against THIS carve's wall, else the
    // analytic ratio (the tessellator's CSG features apply the same rule).
    let cd = select(RECESS_DEPTH, window_info.relief_meta.x / t, window_info.relief_meta.x > 0.0);
    var slope = 0.0;
    var curv = 0.0;
    if (eff == MODE_RIDGE || eff == MODE_TROUGH) {
        // Ridge bump: the carve profile mirrored about the boundary (rising
        // outer half, falling inner half), amplitude halved so the wall tilt
        // matches a step's despite the doubled profile rate. MODE_TROUGH is the
        // same profile inverted — falling outer half, rising inner half — the
        // valley a flush inset control leaves. Sharing this branch is the point:
        // both get ONE evaluation, so neither can drift into the two-pass
        // double-shading the stacked form had.
        let w = clamp(select(2.0 * u, 2.0 - 2.0 * u, u > 0.5), 0.0, 1.0);
        let up = select(-1.0, 1.0, eff == MODE_RIDGE);
        let rising = select(-1.0, 1.0, u <= 0.5) * up;
        slope = rising * 0.5 * cd * 2.0 * carve_slope(w);
        // Each half-wall is a boss wall: concave fillet at its base, convex
        // shoulder toward the crest — and ZERO at the plateaus and crest, so
        // flat ground composites to exactly nothing (a constant term here
        // tints the whole cover quad). A trough's curvature flips with it: the
        // convex shoulders sit at the plateau lips, the concave fillet at the
        // floor.
        curv = -up * rrect_clip.p_mat.w * sin(w * TAU);
    } else {
        let dir = select(-1.0, 1.0, eff == MODE_BOSS);
        // The profile slope is carve_slope's family: smoothstep-derived
        // normally, smootherstep (zero second derivative at the plateaus)
        // under a continuous-curvature corner_shape — shading eases in and out
        // instead of starting on a line.
        slope = dir * cd * carve_slope(u);
        // Curvature: the convex shoulder catches ambient light, the concave
        // fillet self-occludes — on the outer half for a recess, inner for a
        // boss.
        curv = -dir * rrect_clip.p_mat.w * sin(u * TAU);
    }
    let sv = fgd * slope;
    let n = normalize(vec3f(sv, 1.0));
    let diff = PLATE_AMBIENT + (1.0 - PLATE_AMBIENT) * max(dot(n, l), 0.0);
    let spec = roll_spec(sv);
    // Fade the carve out across the host plate's perimeter roll (see p_host).
    let hb = rrect_clip.p_host;
    let host_d = min(hb.z - abs(frag.x - hb.x), hb.w - abs(frag.y - hb.y));
    let att = clamp(host_d / t, 0.0, 1.0) * wedge;
    var v = (diff / flat_shade - 1.0 + curv + spec) * strength * att;
    // p_spec_tint.w = 1 marks a tinted carve — the FOCUS treatment. It
    // renders as the wrapped specular line alone (roll_spec_wrap: the glint
    // the light-facing edges normally carry, swept around the whole
    // outline), matching the focused plates' accent glint exactly; the
    // relief's diffuse/curvature terms drop so a standalone focus ring reads
    // as the line, not a lit step. Plates leave w at 0.
    let tw = rrect_clip.p_spec_tint.w;
    if (tw > 0.0) {
        // The PLATE's monotonic roll profile, not the carve wall's: a wall's
        // slope is a bell (rises then falls), so its tilt crosses the glint
        // angle twice and drew two concentric lines. With roll_slope the ring
        // is exactly a plate silhouette's glint — one line, same position.
        let fr = clamp(1.0 - u, 0.0, 1.0);
        // ...and ENDS at that silhouette (the rect outset by t/2), 1px
        // anti-aliased like a plate's own. Past it `u` saturates at 0 and
        // roll_slope(1) is the profile's steepest point, so every pixel of
        // the cover quad outside the ring drew the full glint — a flat
        // tinted block, square-cornered (the quad's own shape), around the
        // rounded ring.
        let sil = clamp(fd + 0.5 * t + 0.5, 0.0, 1.0);
        v = roll_spec_wrap(fgd * roll_slope(fr)) * strength * att * sil;
    }
    if (v >= 0.0) {
        // Highlight: the white screen mixes toward the tint color, slightly
        // boosted so the accent reads at the rim's low alphas.
        let hl = mix(vec3f(1.0), rrect_clip.p_spec_tint.rgb, tw);
        return vec4f(hl, min(v * (1.0 + 0.5 * tw), 1.0));
    }
    // Shadow: the complementary counter-tint (warm against a cool accent),
    // kept dark (~22%) so it still reads as shadow with a hue cast, not a
    // second glow — the painter's warm-light/cool-shadow trick. Untinted
    // carves stay black.
    let sh = (vec3f(1.0) - rrect_clip.p_spec_tint.rgb) * 0.22 * tw;
    return vec4f(sh, min(-v * (1.0 + 0.5 * tw), 1.0));
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

    // Window-corner coverage: ~1px feather along the squircle silhouette in
    // place of the old hard circular discard, so the window edge, the 3D scene
    // fill, and the plates' tessellated corners all sit on the same curve.
    var clip_cov = 1.0 - smoothstep(-0.5, 0.5, window_corner_distance(in.clip_position.xy));
    if (clip_cov <= 0.0) {
        discard;
    }
    // Circular clip: ~1px feather folded into the coverage (mirroring the
    // rounded-rect clip below) — a clipped edge doubles as the silhouette AA
    // for circle prims drawn as cover quads.
    if (in.clip_circle.z > 0.0) {
        let dx = in.clip_position.x - in.clip_circle.x;
        let dy = in.clip_position.y - in.clip_circle.y;
        let dist = sqrt(dx * dx + dy * dy);
        clip_cov *= 1.0 - smoothstep(in.clip_circle.z - 0.5, in.clip_circle.z + 0.5, dist);
        if (clip_cov <= 0.0) {
            discard;
        }
    }
    // Rounded-rect clip (per-batch): the round-cornered box through rr_sdf_grad,
    // so the clipped silhouette follows the same corner_shape family (rect1.w:
    // circular arc at 2, superellipse squircle above) as the tessellated plate
    // corners around it, with a ~1px feather folded into the fragment alpha in
    // place of the old hard discard — a clipped edge and a drawn plate corner
    // share both curve and AA. Fully-outside fragments still discard.
    if (rrect_clip.rect1.y > 0.5) {
        let r = rrect_clip.rect1.x;
        let prect = vec4f(rrect_clip.rect0.xy, rrect_clip.rect0.zw + vec2f(r, r));
        let d = rr_sdf_grad(in.clip_position.xy, prect, vec4f(r)).z;
        clip_cov *= 1.0 - smoothstep(-0.5, 0.5, d);
        if (clip_cov <= 0.0) {
            discard;
        }
    }

    // SDF-lit plate batch (mode in the push constants; see plate_shade).
    if (i32(round(rrect_clip.rect1.z)) != MODE_NONE) {
        let c = plate_shade(in.clip_position.xy, in.color);
        return vec4f(c.rgb, c.a * clip_cov);
    }

    // A raw negative-alpha vertex with no plate block: geometry pushed from
    // outside the display list (a legacy host's own quads). No recipe to
    // read, so the kernel default and no compression. Everything the
    // display list frosts is a plate batch and never lands here.
    if (in.color.a < 0.0) {
        let c = resolve_blur(in.clip_position.xy, in.color, vec2f(0.0), 0.0, 0.0, LEGACY_STRIDE);
        return vec4f(c.rgb, c.a * clip_cov);
    }

    return vec4f(in.color.rgb, in.color.a * clip_cov);
}

// Blur-behind resolve for a negative-alpha plate color: frosted glass — the
// FULLY blurred backdrop is the base (no clean-backdrop passthrough; mixing
// the clean sample back in at plate opacity left translucent plates barely
// blurred), tinted by the plate color at |alpha| opacity.
//
// `k_in` is the plate's luminance compression and `stride` its kernel's tap
// spacing in physical px (sigma = 2 taps); both come from the plate's own
// push block (MODE_PLATE), or are the no-recipe defaults (droplet, raw
// vertices). A stride of 0 is a CLEAR plate: one clean sample, tinted.
fn resolve_blur(pos: vec2f, color: vec4f, refract: vec2f, clarity: f32, k_in: f32, stride: f32) -> vec4f {
    let tex_size = vec2f(textureDimensions(t_backdrop));

    var backdrop_color = vec4f(0.0);
    if (stride <= 0.0) {
        backdrop_color = textureSample(t_backdrop, s_backdrop, (pos + refract) / tex_size);
    } else {
        var blurred = vec4f(0.0);
        var total_weight = 0.0;
        // 7x7 Gaussian kernel at `stride` px (sigma two taps, reach ±3
        // taps); the linear sampler between taps papers over the stride.
        // The panel default is 5.5 px; a 2.5 px stride was technically a
        // blur but read as plain translucency — fine detail beneath a
        // frosted menu stayed legible, which is not what frosted glass does.
        for (var x = -3.0; x <= 3.0; x += 1.0) {
            for (var y = -3.0; y <= 3.0; y += 1.0) {
                let offset = vec2f(x, y) * stride;
                let sample_uv = (pos + offset) / tex_size;
                let weight = exp(-(x*x + y*y) / (2.0 * 2.0 * 2.0));
                blurred += textureSample(t_backdrop, s_backdrop, sample_uv) * weight;
                total_weight += weight;
            }
        }
        backdrop_color = blurred / total_weight;
    }

    // The rim's clear window onto the backdrop.
    //
    // Refraction has to sample something with STRUCTURE or it is invisible:
    // displacing a field that has already been blurred to sigma ~11px moves
    // smooth values around and reads as nothing at all. So the rim takes a
    // CLEAN sample, displaced by the roll's tilt, and cross-fades to the
    // frosted body — which is also what a real slab does, its thin edge
    // scattering over a shorter path than its thick middle (the droplet
    // branch already trades on that: "thin edges are clearer water").
    //
    // One extra tap, not three: per-channel dispersion inside a band this
    // narrow is invisible once the body blur is 49 taps, and paying for it
    // would triple the most expensive path in this shader to be erased.
    if (clarity > 0.001) {
        let clean = textureSample(t_backdrop, s_backdrop, (pos + refract) / tex_size);
        backdrop_color = mix(backdrop_color, clean, clamp(clarity, 0.0, 1.0));
    }

    let opacity = -color.a;

    // Luminance-range compression, the plate's legibility control.
    //
    // The blur above destroys the backdrop's spatial DETAIL and preserves its
    // mean LUMINANCE — and text contrast is a mean-luminance property, so on
    // its own the mix below hands the backdrop's brightness straight through
    // at (1 - opacity). At the designer dialog's 0.25 that is 75% of whatever
    // is behind it: over the dark viewport a row label runs ~12:1, over
    // something bright ~1.2:1, which is not a contrast ratio so much as its
    // absence. No amount of extra blur moves either number.
    //
    // So remap the backdrop's luminance toward the plate's own key, keeping
    // its chromaticity. This is not "darken" and not opacity: it is
    // SYMMETRIC, pulling a bright backdrop down and a dark one UP, so what it
    // removes is the plate's swing through the ink's luminance rather than
    // the view through it. Hue, chroma and movement all still read.
    // Compression is a LEGIBILITY control and the rim carries no text, so the
    // clear window opened there is exempt in proportion to how clear it is.
    // Tone-mapping it would pull the refracted view back toward the plate's
    // own key — the exact contrast the rim exists to show — and the effect
    // measured nearly invisible with the two fighting.
    let k = clamp(k_in, 0.0, 1.0) * (1.0 - clamp(clarity, 0.0, 1.0));
    let W = vec3f(0.2126, 0.7152, 0.0722);
    let bl = dot(backdrop_color.rgb, W);
    let key = dot(color.rgb, W);
    // `target` is a WGSL reserved word.
    let keyed = mix(bl, key, k);
    // Scaling by keyed/bl holds chromaticity exactly, but near black the
    // ratio explodes (and a lift past 1 would clip a channel), so cross-fade
    // to the neutral luminance over the bottom of the range instead.
    let scaled = backdrop_color.rgb * (keyed / max(bl, 1e-4));
    let compressed = mix(vec3f(keyed), scaled, smoothstep(0.0, 0.05, bl));

    return vec4f(mix(compressed, color.rgb, opacity), 1.0);
}
