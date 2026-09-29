// rt_common.wgsl — the path tracer's shared core (RT-renderer phases 2+4).
//
// Everything except the trace call: params, scene/material buffers,
// accumulation, RNG, sky, sampling, and cs_main. Binding 1 and
// `intersect_scene` come from whichever tier file is concatenated after
// this one at pipeline creation:
//   - rt_bvh.wgsl   — tier 1: a CPU-built BVH traversed in compute; runs
//                     on any device, no VK_KHR_ray_* required.
//   - rt_query.wgsl — tier 2: hardware ray queries against a driver-built
//                     TLAS (VK_KHR_ray_query), engaging RT cores.
// One dispatch adds `spp` samples per pixel into the accumulation buffer
// (progressive refinement); the running mean is tone-mapped (clamped
// linear) into `out_img`, which the stage blits into the backdrop pane.

struct Params {
    // Inverse of the raster path's proj*view*model: unprojects wgpu-style NDC
    // (y up, z in [0,1]) into mesh space, so rays live in the same space as
    // the triangles fed to `set_rt_scene`.
    inv_mvp: mat4x4<f32>,
    width: u32,
    height: u32,
    sample_index: u32,
    max_bounces: u32,
    // Samples per dispatch: 1 for the interactive viewport (one refinement
    // step per frame), higher for offscreen/thumbnail rendering so a whole
    // image needs only a few submits.
    spp: u32,
    _pad0: u32,
    _pad1: u32,
    _pad2: u32,
    // The scene's image, a quad of two triangles whose material is marked
    // textured (albedo.w). xyz = its top-left corner; w = its opacity, 0
    // when there is no image to sample (a textured hit then lets the ray
    // through).
    img_origin: vec4<f32>,
    // xyz = the top edge, corner to corner; w = the texture's width in
    // texels.
    img_u: vec4<f32>,
    // xyz = the left edge, top to bottom; w = the texture's height.
    img_v: vec4<f32>,
}

@group(0) @binding(0) var<uniform> params: Params;

// Binding 1 belongs to the tier file: the BVH node buffer (tier 1) or the
// acceleration structure (tier 2).

// Positions in xyz; p0.w carries the material index (bitcast).
struct Tri {
    p0: vec4<f32>,
    p1: vec4<f32>,
    p2: vec4<f32>,
}
@group(0) @binding(2) var<storage, read> tris: array<Tri>;

struct Material {
    albedo: vec4<f32>,
    emission: vec4<f32>,
}
@group(0) @binding(3) var<storage, read> materials: array<Material>;

// One vec4 per pixel: rgb = radiance sum, a = sample count.
@group(0) @binding(4) var<storage, read_write> accum: array<vec4<f32>>;

@group(0) @binding(5) var out_img: texture_storage_2d<rgba8unorm, write>;

// Primary-hit features for the denoiser (rt_denoise.wgsl), two vec4s per
// pixel: [2i] = (shading normal, hit t — 1e30 for sky), [2i+1] = (albedo, 0).
@group(0) @binding(6) var<storage, read_write> features: array<vec4<f32>>;

// The scene's image and its sampler. Always bound: to a 1x1 stand-in while
// the scene has no image.
@group(0) @binding(7) var img: texture_2d<f32>;
@group(0) @binding(8) var img_sampler: sampler;

// PCG (O'Neill) — one u32 of state per path, advanced per draw.
fn rand(state: ptr<function, u32>) -> f32 {
    var s = *state * 747796405u + 2891336453u;
    *state = s;
    let word = ((s >> ((s >> 28u) + 4u)) ^ s) * 277803737u;
    return f32((word >> 22u) ^ word) * (1.0 / 4294967295.0);
}

// The tier boundary: whichever tier file follows provides
//   fn intersect_scene(ro: vec3<f32>, rd: vec3<f32>) -> HitInfo
struct HitInfo {
    t: f32,
    tri: u32,
}

// A soft studio sky: vertical gradient plus one warm key light. This is the
// only light source until emissive geometry shows up in scenes.
fn sky(rd: vec3<f32>) -> vec3<f32> {
    let t = clamp(rd.y * 0.5 + 0.5, 0.0, 1.0);
    var s = mix(vec3<f32>(0.32, 0.31, 0.35), vec3<f32>(0.72, 0.82, 0.98), t);
    let sun = normalize(vec3<f32>(0.45, 0.75, 0.35));
    s = s + vec3<f32>(1.0, 0.95, 0.85) * pow(max(dot(rd, sun), 0.0), 48.0) * 8.0;
    return s;
}

fn cosine_dir(n: vec3<f32>, r1: f32, r2: f32) -> vec3<f32> {
    let a = 6.28318530718 * r1;
    let r = sqrt(r2);
    var up = vec3<f32>(1.0, 0.0, 0.0);
    if abs(n.x) > 0.5 {
        up = vec3<f32>(0.0, 1.0, 0.0);
    }
    let tangent = normalize(cross(n, up));
    let bitangent = cross(n, tangent);
    return normalize(
        tangent * (r * cos(a)) + bitangent * (r * sin(a)) + n * sqrt(max(0.0, 1.0 - r2)),
    );
}

@compute @workgroup_size(8, 8)
fn cs_main(@builtin(global_invocation_id) gid: vec3<u32>) {
    if gid.x >= params.width || gid.y >= params.height {
        return;
    }
    let idx = gid.y * params.width + gid.x;

    var total = vec3<f32>(0.0);
    for (var s: u32 = 0u; s < params.spp; s = s + 1u) {
        var rng: u32 = (idx * 9781u) ^ ((params.sample_index + s) * 26699u) ^ 0x9e3779b9u;

        // Jittered primary ray, unprojected through inv_mvp (NDC y up, z 0..1).
        let jx = rand(&rng);
        let jy = rand(&rng);
        let ndc_x = (f32(gid.x) + jx) / f32(params.width) * 2.0 - 1.0;
        let ndc_y = 1.0 - (f32(gid.y) + jy) / f32(params.height) * 2.0;
        let p_near = params.inv_mvp * vec4<f32>(ndc_x, ndc_y, 0.0, 1.0);
        let p_far = params.inv_mvp * vec4<f32>(ndc_x, ndc_y, 1.0, 1.0);
        var ro = p_near.xyz / p_near.w;
        var rd = normalize(p_far.xyz / p_far.w - ro);
        // The angle one pixel subtends, for the image's mip level: the
        // ray through the next pixel along, against this one.
        let p_next = params.inv_mvp
            * vec4<f32>(ndc_x + 2.0 / f32(params.width), ndc_y, 1.0, 1.0);
        let pixel_angle = length(normalize(p_next.xyz / p_next.w - ro) - rd);
        let eye = ro;

        var radiance = vec3<f32>(0.0);
        var throughput = vec3<f32>(1.0);
        // Until the path first lands on something or leaves for the sky:
        // what it lands on is the pixel's feature for the denoiser. Not
        // "the first bounce" — a ray let through the image's clear texels
        // has used one and landed on nothing.
        var primary = s == 0u;
        // Until the path first scatters it is the camera's own ray, and a
        // pixel's footprint on what it hits is known.
        var straight = true;
        for (var bounce: u32 = 0u; bounce < params.max_bounces; bounce = bounce + 1u) {
            let hit = intersect_scene(ro, rd);
            if hit.t >= 1e30 {
                if primary {
                    features[2u * idx] = vec4<f32>(0.0, 0.0, 0.0, 1e30);
                    features[2u * idx + 1u] = vec4<f32>(1.0, 1.0, 1.0, 0.0);
                }
                radiance = radiance + throughput * sky(rd);
                break;
            }
            let tri = tris[hit.tri];
            let mat = materials[bitcast<u32>(tri.p0.w)];
            let at = ro + rd * hit.t;
            var albedo = mat.albedo.rgb;
            if mat.albedo.w > 0.5 {
                // The image: its colour is the surface's, and where it is
                // clear the ray goes on as if nothing were there — by
                // chance, in proportion, which over the samples is the
                // image's own alpha.
                let rel = at - params.img_origin.xyz;
                let u = params.img_u.xyz;
                let v = params.img_v.xyz;
                let uv = vec2<f32>(dot(rel, u) / dot(u, u), dot(rel, v) / dot(v, v));
                // The level whose texel is a pixel's footprint wide. By the
                // footprint's SHORT axis: seen at a slant the long one is
                // averaged by the samples, where a level chosen for it
                // would blur both. A scattered ray has no footprint and
                // takes a coarse level.
                var lod = 3.0;
                if straight {
                    let footprint = length(at - eye) * pixel_angle;
                    let texel = length(u) / max(params.img_u.w, 1.0);
                    lod = max(log2(footprint / max(texel, 1e-12)), 0.0);
                }
                let texel = textureSampleLevel(img, img_sampler, uv, lod);
                if rand(&rng) >= texel.a * params.img_origin.w {
                    ro = at + rd * (1e-4 * max(1.0, hit.t));
                    continue;
                }
                albedo = texel.rgb;
            }
            radiance = radiance + throughput * mat.emission.rgb;
            var n = normalize(cross(tri.p1.xyz - tri.p0.xyz, tri.p2.xyz - tri.p0.xyz));
            if dot(n, rd) > 0.0 {
                n = -n;
            }
            if primary {
                features[2u * idx] = vec4<f32>(n, length(at - eye));
                features[2u * idx + 1u] = vec4<f32>(albedo, 0.0);
            }
            primary = false;
            straight = false;
            throughput = throughput * albedo;
            ro = at + n * 1e-4;
            rd = cosine_dir(n, rand(&rng), rand(&rng));
        }
        // Firefly clamp: rare sun-spike paths otherwise leave speckles the
        // variance can't average out (and the denoiser's edge-stopping
        // weights deliberately refuse to smear). Slight energy loss on
        // extreme highlights, big variance win.
        total = total + min(radiance, vec3<f32>(4.0));
    }

    var acc = accum[idx];
    if params.sample_index == 0u {
        acc = vec4<f32>(0.0);
    }
    acc = acc + vec4<f32>(total, f32(params.spp));
    accum[idx] = acc;
    let color = acc.rgb / max(acc.a, 1.0);
    textureStore(
        out_img,
        vec2<i32>(i32(gid.x), i32(gid.y)),
        vec4<f32>(clamp(color, vec3<f32>(0.0), vec3<f32>(1.0)), 1.0),
    );
}
