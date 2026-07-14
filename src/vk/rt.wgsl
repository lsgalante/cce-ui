// rt.wgsl — the tier-1 compute path tracer (RT-renderer phase 2).
//
// Pure Vulkan compute: a CPU-built BVH (see rt.rs) is traversed per ray, so
// this runs on any device — no VK_KHR_ray_* required. One dispatch adds one
// sample per pixel into the accumulation buffer (progressive refinement);
// the running mean is tone-mapped (clamped linear) into `out_img`, which the
// stage blits into the backdrop pane. Geometry, camera, and accumulation are
// deliberately independent of the trace call so a tier-2 ray-query backend
// can swap in `intersect_scene` later.

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
}

@group(0) @binding(0) var<uniform> params: Params;

// 32-byte BVH node (rt.rs `GpuBvhNode`): count > 0 marks a leaf over
// tris[left_first .. left_first+count]; otherwise children are at
// left_first and left_first + 1.
struct Node {
    bmin: vec3<f32>,
    left_first: u32,
    bmax: vec3<f32>,
    count: u32,
}
@group(0) @binding(1) var<storage, read> nodes: array<Node>;

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

// PCG (O'Neill) — one u32 of state per path, advanced per draw.
fn rand(state: ptr<function, u32>) -> f32 {
    var s = *state * 747796405u + 2891336453u;
    *state = s;
    let word = ((s >> ((s >> 28u) + 4u)) ^ s) * 277803737u;
    return f32((word >> 22u) ^ word) * (1.0 / 4294967295.0);
}

// Slab test: entry distance, or 1e30 on miss / beyond the current hit.
fn intersect_aabb(
    ro: vec3<f32>,
    inv_rd: vec3<f32>,
    bmin: vec3<f32>,
    bmax: vec3<f32>,
    t_limit: f32,
) -> f32 {
    let t1 = (bmin - ro) * inv_rd;
    let t2 = (bmax - ro) * inv_rd;
    let lo = min(t1, t2);
    let hi = max(t1, t2);
    let tn = max(max(lo.x, lo.y), lo.z);
    let tf = min(min(hi.x, hi.y), hi.z);
    if tf >= max(tn, 0.0) && tn < t_limit {
        return tn;
    }
    return 1e30;
}

// Möller–Trumbore, two-sided (scene triangles have no guaranteed winding).
fn intersect_tri(ro: vec3<f32>, rd: vec3<f32>, i: u32, t_limit: f32) -> f32 {
    let tri = tris[i];
    let e1 = tri.p1.xyz - tri.p0.xyz;
    let e2 = tri.p2.xyz - tri.p0.xyz;
    let h = cross(rd, e2);
    let a = dot(e1, h);
    if abs(a) < 1e-8 {
        return 1e30;
    }
    let f = 1.0 / a;
    let s = ro - tri.p0.xyz;
    let u = f * dot(s, h);
    if u < 0.0 || u > 1.0 {
        return 1e30;
    }
    let q = cross(s, e1);
    let v = f * dot(rd, q);
    if v < 0.0 || u + v > 1.0 {
        return 1e30;
    }
    let t = f * dot(e2, q);
    if t > 1e-4 && t < t_limit {
        return t;
    }
    return 1e30;
}

struct HitInfo {
    t: f32,
    tri: u32,
}

// Ordered BVH traversal with an explicit stack. THE tier boundary: a
// ray-query backend replaces exactly this function.
fn intersect_scene(ro: vec3<f32>, rd: vec3<f32>) -> HitInfo {
    var hit = HitInfo(1e30, 0u);
    if arrayLength(&nodes) == 0u {
        return hit;
    }
    let inv_rd = vec3<f32>(1.0, 1.0, 1.0) / rd;
    var stack: array<u32, 32>;
    var sp: u32 = 0u;
    var node_idx: u32 = 0u;
    if intersect_aabb(ro, inv_rd, nodes[0].bmin, nodes[0].bmax, hit.t) >= 1e30 {
        return hit;
    }
    loop {
        let node = nodes[node_idx];
        if node.count > 0u {
            for (var i: u32 = 0u; i < node.count; i = i + 1u) {
                let tri_idx = node.left_first + i;
                let t = intersect_tri(ro, rd, tri_idx, hit.t);
                if t < hit.t {
                    hit.t = t;
                    hit.tri = tri_idx;
                }
            }
            if sp == 0u {
                break;
            }
            sp = sp - 1u;
            node_idx = stack[sp];
            continue;
        }
        // Internal: visit the nearer child first, defer the farther one.
        var near = node.left_first;
        var far = node.left_first + 1u;
        var t_near = intersect_aabb(ro, inv_rd, nodes[near].bmin, nodes[near].bmax, hit.t);
        var t_far = intersect_aabb(ro, inv_rd, nodes[far].bmin, nodes[far].bmax, hit.t);
        if t_far < t_near {
            let tmp_i = near;
            near = far;
            far = tmp_i;
            let tmp_t = t_near;
            t_near = t_far;
            t_far = tmp_t;
        }
        if t_near >= 1e30 {
            if sp == 0u {
                break;
            }
            sp = sp - 1u;
            node_idx = stack[sp];
            continue;
        }
        if t_far < 1e30 && sp < 32u {
            stack[sp] = far;
            sp = sp + 1u;
        }
        node_idx = near;
    }
    return hit;
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

        var radiance = vec3<f32>(0.0);
        var throughput = vec3<f32>(1.0);
        for (var bounce: u32 = 0u; bounce < params.max_bounces; bounce = bounce + 1u) {
            let hit = intersect_scene(ro, rd);
            if hit.t >= 1e30 {
                radiance = radiance + throughput * sky(rd);
                break;
            }
            let tri = tris[hit.tri];
            let mat = materials[bitcast<u32>(tri.p0.w)];
            radiance = radiance + throughput * mat.emission.rgb;
            var n = normalize(cross(tri.p1.xyz - tri.p0.xyz, tri.p2.xyz - tri.p0.xyz));
            if dot(n, rd) > 0.0 {
                n = -n;
            }
            throughput = throughput * mat.albedo.rgb;
            ro = ro + rd * hit.t + n * 1e-4;
            rd = cosine_dir(n, rand(&rng), rand(&rng));
        }
        total = total + radiance;
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
